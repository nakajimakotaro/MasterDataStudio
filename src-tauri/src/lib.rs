use gamemasterstudio_core::{
    project::{Operation, Project, ProjectUpdate, RepositoryState, Snapshot},
    storage,
};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Mutex};
use tauri::{Manager, State};

#[derive(Default)]
struct AppState(
    Mutex<Option<Project>>,
    Mutex<Option<(String, gamemasterstudio_core::history::HistoryDetail)>>,
);

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalSettings {
    recent_projects: Vec<String>,
}

fn local_settings_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json"))
}

#[tauri::command(async)]
fn recent_projects(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let settings = storage::read_optional(&local_settings_path(&app)?)?
        .map(|bytes| serde_json::from_slice::<LocalSettings>(&bytes).map_err(|e| e.to_string()))
        .transpose()?
        .unwrap_or_default();
    Ok(settings.recent_projects)
}

fn remember(app: &tauri::AppHandle, root: &str) -> Result<(), String> {
    let mut recent = recent_projects(app.clone())?;
    recent.retain(|p| p != root);
    recent.insert(0, root.into());
    recent.truncate(12);
    let mut bytes = serde_json::to_vec_pretty(&LocalSettings {
        recent_projects: recent,
    })
    .map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    storage::atomic_write(&local_settings_path(app)?, &bytes)
}

#[tauri::command(async)]
fn open_project(
    path: String,
    initialize: bool,
    safe_mode: Option<bool>,
    app: tauri::AppHandle,
    state: State<AppState>,
) -> Result<Snapshot, String> {
    let mut current = state.0.lock().map_err(|e| e.to_string())?;
    let mut project = if initialize {
        Project::initialize(Path::new(&path))?
    } else {
        Project::open(Path::new(&path))?
    };
    project.safe_mode = safe_mode.unwrap_or(false);
    let snapshot = project.snapshot();
    // A failed local preference write must not turn a successful repository open into a failure.
    if let Err(error) = remember(&app, &snapshot.root) {
        eprintln!("Recent projects: {error}");
    }
    *state.1.lock().map_err(|e| e.to_string())? = None;
    *current = Some(project);
    Ok(snapshot)
}

#[tauri::command(async)]
fn clone_project(
    url: String,
    path: String,
    safe_mode: Option<bool>,
    app: tauri::AppHandle,
    state: State<AppState>,
) -> Result<Snapshot, String> {
    let mut current = state.0.lock().map_err(|e| e.to_string())?;
    let mut project = Project::clone_repository(&url, Path::new(&path))?;
    project.safe_mode = safe_mode.unwrap_or(false);
    let snapshot = project.snapshot();
    if let Err(error) = remember(&app, &snapshot.root) {
        eprintln!("Recent projects: {error}");
    }
    *state.1.lock().map_err(|e| e.to_string())? = None;
    *current = Some(project);
    Ok(snapshot)
}

#[tauri::command(async)]
fn close_project(state: State<AppState>) -> Result<(), String> {
    *state.0.lock().map_err(|e| e.to_string())? = None;
    *state.1.lock().map_err(|e| e.to_string())? = None;
    Ok(())
}

// Repository work and waits for this lock run off the UI thread.
fn with_project<T>(
    state: State<AppState>,
    f: impl FnOnce(&mut Project) -> Result<T, String>,
) -> Result<T, String> {
    let mut state = state.0.lock().map_err(|e| e.to_string())?;
    f(state.as_mut().ok_or("Repository を開いてください。")?)
}

#[tauri::command(async)]
fn get_project(state: State<AppState>) -> Result<Snapshot, String> {
    with_project(state, |p| Ok(p.snapshot()))
}

#[tauri::command(async)]
fn repository_state(
    root: String,
    revision: u64,
    state: State<AppState>,
) -> Result<RepositoryState, String> {
    with_project(state, |p| {
        let result = p.repository_state()?;
        if result.root != root || result.revision != revision {
            return Err("編集状態が更新されました。もう一度操作してください。".into());
        }
        Ok(result)
    })
}

#[tauri::command(async)]
fn edit_project(
    operation: Operation,
    calculated: Option<Vec<gamemasterstudio_core::scripts::CalculatedCell>>,
    revision: u64,
    state: State<AppState>,
) -> Result<ProjectUpdate, String> {
    with_project(state, |p| {
        p.apply_calculated_update(operation, calculated.unwrap_or_default(), revision)
    })
}

#[tauri::command(async)]
fn preview_edit(
    operation: Operation,
    revision: u64,
    state: State<AppState>,
) -> Result<gamemasterstudio_core::scripts::PreparedEdit, String> {
    let current = state.0.lock().map_err(|e| e.to_string())?;
    current
        .as_ref()
        .ok_or("Repository を開いてください。")?
        .preview_scripts(operation, revision)
}

#[tauri::command(async)]
fn set_identity(name: String, email: String, state: State<AppState>) -> Result<Snapshot, String> {
    with_project(state, |p| p.set_identity(&name, &email))
}

#[tauri::command(async)]
fn git_fetch(state: State<AppState>) -> Result<Snapshot, String> {
    with_project(state, |p| p.fetch())
}
#[tauri::command(async)]
fn git_update(state: State<AppState>) -> Result<Snapshot, String> {
    with_project(state, |p| p.update())
}
#[tauri::command(async)]
fn git_push(state: State<AppState>) -> Result<Snapshot, String> {
    with_project(state, |p| p.push())
}
#[tauri::command(async)]
fn switch_branch(branch: String, create: bool, state: State<AppState>) -> Result<Snapshot, String> {
    with_project(state, |p| p.switch_branch(&branch, create))
}
#[tauri::command(async)]
fn commit(message: String, push: bool, state: State<AppState>) -> Result<Snapshot, String> {
    with_project(state, |p| p.commit(&message, push))
}
#[tauri::command(async)]
fn revert_change(
    change: gamemasterstudio_core::project::SemanticChange,
    revision: u64,
    state: State<AppState>,
) -> Result<Snapshot, String> {
    with_project(state, |p| p.revert_change(change, revision))
}

#[tauri::command(async)]
fn merge_branch(branch: String, state: State<AppState>) -> Result<Snapshot, String> {
    with_project(state, |p| p.merge_branch(&branch))
}
#[tauri::command(async)]
fn resolve_conflict(
    id: String,
    resolution: gamemasterstudio_core::merge::Resolution,
    revision: u64,
    state: State<AppState>,
) -> Result<Snapshot, String> {
    with_project(state, |p| p.resolve_conflict(id, resolution, revision))
}
#[tauri::command(async)]
fn resolve_conflicts(
    ids: Vec<String>,
    resolution: gamemasterstudio_core::merge::Resolution,
    revision: u64,
    state: State<AppState>,
) -> Result<Snapshot, String> {
    with_project(state, |p| p.resolve_conflicts(ids, resolution, revision))
}
#[tauri::command(async)]
fn complete_merge(
    message: String,
    revision: u64,
    state: State<AppState>,
) -> Result<Snapshot, String> {
    with_project(state, |p| p.complete_merge(&message, revision))
}
#[tauri::command(async)]
fn abort_merge(state: State<AppState>) -> Result<Snapshot, String> {
    with_project(state, |p| p.abort_merge())
}

#[tauri::command(async)]
fn project_history(
    root: String,
    cursor: Option<String>,
    query: String,
    author: String,
    state: State<AppState>,
) -> Result<gamemasterstudio_core::history::HistoryPage, String> {
    let state = state.0.lock().map_err(|e| e.to_string())?;
    let project = state.as_ref().ok_or("Repository を開いてください。")?;
    if project.root_path().to_string_lossy() != root {
        return Err("Project が切り替わりました。".into());
    }
    project.history_search(cursor.as_deref(), &query, &author)
}
#[tauri::command(async)]
fn history_detail(
    root: String,
    oid: String,
    filter: gamemasterstudio_core::history::ChangeFilter,
    state: State<AppState>,
) -> Result<gamemasterstudio_core::history::HistoryChangesPage, String> {
    let project_guard = state.0.lock().map_err(|e| e.to_string())?;
    let project = project_guard
        .as_ref()
        .ok_or("Repository を開いてください。")?;
    if project.root_path().to_string_lossy() != root {
        return Err("Project が切り替わりました。".into());
    }
    let mut cache = state.1.lock().map_err(|e| e.to_string())?;
    if !cache
        .as_ref()
        .is_some_and(|(r, d)| r == &root && d.oid == oid)
    {
        *cache = Some((root, project.history_detail(&oid)?));
    }
    Ok(cache.as_ref().unwrap().1.page(&filter))
}

#[tauri::command(async)]
fn script_masters(state: State<AppState>) -> Result<Vec<String>, String> {
    with_project(state, |p| p.script_masters())
}

#[tauri::command(async)]
fn read_master(
    root: String,
    master_id: String,
    revision: u64,
    state: State<AppState>,
) -> Result<gamemasterstudio_core::project::MasterEntry, String> {
    with_project(state, |p| {
        if p.root_path().to_string_lossy() != root {
            return Err("Project が切り替わりました。".into());
        }
        p.master(&master_id, revision)
    })
}

#[tauri::command(async)]
fn review_summary(
    root: String,
    revision: u64,
    state: State<AppState>,
) -> Result<gamemasterstudio_core::project::ReviewSummary, String> {
    with_project(state, |p| {
        if p.root_path().to_string_lossy() != root {
            return Err("Project が切り替わりました。".into());
        }
        p.review_summary(revision)
    })
}

#[tauri::command(async)]
fn change_review(
    root: String,
    master_id: String,
    revision: u64,
    state: State<AppState>,
) -> Result<gamemasterstudio_core::project::ChangeReview, String> {
    let state = state.0.lock().map_err(|e| e.to_string())?;
    let project = state.as_ref().ok_or("Repository を開いてください。")?;
    if project.root_path().to_string_lossy() != root {
        return Err("Project が切り替わりました。".into());
    }
    project.review_master(revision, &master_id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            recent_projects,
            open_project,
            clone_project,
            close_project,
            get_project,
            edit_project,
            preview_edit,
            set_identity,
            git_fetch,
            git_update,
            git_push,
            switch_branch,
            commit,
            revert_change,
            merge_branch,
            resolve_conflict,
            resolve_conflicts,
            complete_merge,
            abort_merge,
            project_history,
            history_detail,
            change_review,
            read_master,
            script_masters,
            review_summary,
            repository_state
        ])
        .run(tauri::generate_context!())
        .expect("GameMasterStudio could not start");
}
