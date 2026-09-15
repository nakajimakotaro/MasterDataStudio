use crate::{
    comments::{CommentTarget, Comments, Identity},
    config::{validate_column, GitConfig, MasterDefinition, ProjectConfig, CONFIG_PATH},
    csv_data::{PrimaryKey, Table},
    storage::{self, FileChange},
    Result,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Master {
    pub table: Table,
    pub comments: Comments,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasterEntry {
    pub data: Option<Master>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectData {
    pub config: ProjectConfig,
    pub masters: BTreeMap<String, MasterEntry>,
}

#[derive(Serialize)]
pub struct ChangeReview {
    pub before: Option<ProjectData>,
    pub after: ProjectData,
    pub changes: Vec<SemanticChange>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub root: String,
    pub name: String,
    pub identity: Identity,
    pub data: ProjectData,
    pub can_undo: bool,
    pub can_redo: bool,
    pub revision: u64,
    pub git: GitStatus,
    pub changes: Vec<SemanticChange>,
    pub changes_error: Option<String>,
    pub merge: Option<crate::merge::MergeView>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub protected: bool,
    pub tracked_dirty: bool,
    pub merge_in_progress: bool,
    pub remotes: Vec<String>,
    pub branches: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SemanticChange {
    ProjectConfig {
        master_id: String,
        before: Option<GitConfig>,
        after: GitConfig,
    },
    MasterDefinition {
        master_id: String,
        before: MasterDefinition,
        after: MasterDefinition,
    },
    AddedMaster {
        master_id: String,
    },
    DeletedMaster {
        master_id: String,
    },
    AddedColumn {
        master_id: String,
        column: String,
    },
    DeletedColumn {
        master_id: String,
        column: String,
    },
    AddedRow {
        master_id: String,
        primary_key: PrimaryKey,
    },
    DeletedRow {
        master_id: String,
        primary_key: PrimaryKey,
    },
    Cell {
        master_id: String,
        primary_key: PrimaryKey,
        column: String,
        before: String,
        after: String,
    },
    Comment {
        master_id: String,
        target: CommentTarget,
        before: Option<String>,
        after: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellEdit {
    pub primary_key: PrimaryKey,
    pub column: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Operation {
    SetProtectedBranches {
        patterns: Vec<String>,
    },
    EditCells {
        master_id: String,
        edits: Vec<CellEdit>,
    },
    CreateRows {
        master_id: String,
        rows: Vec<Vec<String>>,
    },
    AddRow {
        master_id: String,
        primary_key: PrimaryKey,
        duplicate_from: Option<PrimaryKey>,
    },
    DeleteRows {
        master_id: String,
        primary_keys: Vec<PrimaryKey>,
    },
    AddColumn {
        master_id: String,
        name: String,
    },
    DeleteColumn {
        master_id: String,
        name: String,
    },
    SetComment {
        master_id: String,
        target: CommentTarget,
        body: String,
    },
    CreateMaster {
        master_id: String,
        path: String,
        primary_key: Vec<String>,
        columns: Vec<String>,
    },
    ConfigureMaster {
        master_id: String,
        path: String,
        primary_key: Vec<String>,
    },
}

pub struct Project {
    pub(crate) merge: Option<crate::merge_git::MergeSession>,
    pub(crate) root: PathBuf,
    pub(crate) data: ProjectData,
    pub(crate) identity: Identity,
    pub(crate) undo: Vec<ProjectData>,
    pub(crate) redo: Vec<ProjectData>,
    pub(crate) revision: u64,
}

pub fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("Git を実行できません: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn repository_root(path: &Path) -> Result<PathBuf> {
    let top = git(path, &["rev-parse", "--show-toplevel"])?;
    fs::canonicalize(top).map_err(|e| e.to_string())
}

fn identity(root: &Path) -> Identity {
    Identity {
        name: git(root, &["config", "user.name"]).unwrap_or_default(),
        email: git(root, &["config", "user.email"]).unwrap_or_default(),
    }
}

impl Project {
    pub fn root_path(&self) -> &Path {
        &self.root
    }

    pub fn open(path: &Path) -> Result<Self> {
        let root = repository_root(path)?;
        if git(&root, &["rev-parse", "--verify", "MERGE_HEAD"]).is_ok() {
            let data = crate::merge_git::read_source(&root, "HEAD")?;
            let merge = Some(crate::merge_git::MergeSession::recover(&root));
            return Ok(Self {
                identity: identity(&root),
                root,
                data,
                merge,
                undo: vec![],
                redo: vec![],
                revision: 0,
            });
        }
        let config_path = storage::safe_path(&root, CONFIG_PATH)?;
        let bytes = storage::read_optional(&config_path)?
            .ok_or("Project Config がありません。「Project を初期化」を使用してください。")?;
        let config = ProjectConfig::parse(&bytes)?;
        let mut masters = BTreeMap::new();
        for (id, def) in &config.masters {
            let load = || -> Result<Master> {
                let path = storage::safe_path(&root, &def.path)?;
                let table = Table::parse(
                    &fs::read(path).map_err(|e| format!("{}: {e}", def.path))?,
                    def,
                )?;
                let comment_path =
                    storage::safe_path(&root, &format!("gamemasterstudio/comments/{id}.json"))?;
                let mut comments = match storage::read_optional(&comment_path)? {
                    Some(bytes) => serde_json::from_slice::<Comments>(&bytes)
                        .map_err(|e| format!("Comment JSON: {e}"))?,
                    None => Comments::default(),
                };
                comments.validate(&table, def)?;
                Ok(Master { table, comments })
            };
            masters.insert(
                id.clone(),
                match load() {
                    Ok(data) => MasterEntry {
                        data: Some(data),
                        error: None,
                    },
                    Err(error) => MasterEntry {
                        data: None,
                        error: Some(error),
                    },
                },
            );
        }
        Ok(Self {
            merge: None,
            identity: identity(&root),
            root,
            data: ProjectData { config, masters },
            undo: vec![],
            redo: vec![],
            revision: 0,
        })
    }

    pub fn initialize(path: &Path) -> Result<Self> {
        if !path.is_dir() {
            return Err("既存のフォルダを選択してください。".into());
        }
        let selected = fs::canonicalize(path).map_err(|e| e.to_string())?;
        let root = match repository_root(&selected) {
            Ok(root) => root,
            Err(_) => {
                git(&selected, &["init", "-b", "main"])?;
                selected
            }
        };
        let config_path = storage::safe_path(&root, CONFIG_PATH)?;
        if config_path.exists() {
            return Err(
                "Project は初期化済みです。「Repository を開く」を使用してください。".into(),
            );
        }
        let mut config = ProjectConfig::default();
        if let Ok(default) = git(
            &root,
            &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        ) {
            if let Some(branch) = default.strip_prefix("origin/") {
                config.git.protected_branches = vec![branch.into()];
            }
        }
        storage::atomic_write(&config_path, &config.serialize()?)?;
        Self::open(&root)
    }

    pub fn snapshot(&self) -> Snapshot {
        let git = self.git_status().unwrap_or_default();
        let changes = self.semantic_diff();
        let changes_error = changes.as_ref().err().cloned();
        let changes = changes.unwrap_or_default();
        Snapshot {
            root: self.root.to_string_lossy().into_owned(),
            name: self
                .root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            identity: self.identity.clone(),
            data: self.data.clone(),
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            revision: self.revision,
            git,
            changes,
            changes_error,
            merge: self.merge.as_ref().map(|m| m.view()),
        }
    }

    pub fn set_identity(&mut self, name: &str, email: &str) -> Result<Snapshot> {
        let next = Identity {
            name: name.trim().into(),
            email: email.trim().into(),
        };
        if !next.is_complete()
            || next.name.contains(['\r', '\n', '\0'])
            || next.email.contains(['\r', '\n', '\0'])
        {
            return Err("Git user.name / user.email の両方を入力してください。".into());
        }
        git(&self.root, &["config", "--local", "user.name", &next.name])?;
        let result = git(
            &self.root,
            &["config", "--local", "user.email", &next.email],
        );
        self.identity = identity(&self.root);
        result?;
        Ok(self.snapshot())
    }

    fn writable(&self, revision: u64) -> Result<()> {
        if revision != self.revision {
            return Err("編集状態が更新されました。もう一度操作してください。".into());
        }
        if !self.identity.is_complete() {
            return Err(
                "編集するには Project Settings で Git Identity を設定してください。".into(),
            );
        }
        if self.git_status()?.merge_in_progress {
            return Err("Merge 中は Conflict Resolver で解決してください。".into());
        }
        if self.git_status()?.protected {
            return Err(
                "Protected Branch では編集できません。Working Branch を作成してください。".into(),
            );
        }
        Ok(())
    }

    pub fn apply(&mut self, operation: Operation, revision: u64) -> Result<Snapshot> {
        self.writable(revision)?;
        let mut next = self.data.clone();
        match operation {
            Operation::SetProtectedBranches { patterns } => {
                next.config.git.protected_branches = patterns;
                next.config.validate()?;
                let branch = self.git_status()?.branch;
                if next
                    .config
                    .git
                    .protected_branches
                    .iter()
                    .any(|p| glob_matches(p, &branch))
                {
                    return Err("現在の Branch を保護すると設定変更を Commit できません。保護対象外の Working Branch で変更してください。".into());
                }
            }
            Operation::CreateMaster {
                master_id,
                path,
                primary_key,
                columns,
            } => {
                if next.config.masters.contains_key(&master_id) {
                    return Err("Master ID が重複しています。".into());
                }
                next.config
                    .masters
                    .insert(master_id.clone(), MasterDefinition { path, primary_key });
                next.config.validate()?;
                let def = &next.config.masters[&master_id];
                if storage::safe_path(&self.root, &def.path)?.exists()
                    || storage::safe_path(
                        &self.root,
                        &format!("gamemasterstudio/comments/{master_id}.json"),
                    )?
                    .exists()
                {
                    return Err(
                        "保存先に既存ファイルがあります。別の path / ID を指定してください。"
                            .into(),
                    );
                }
                let mut table = Table {
                    columns,
                    rows: vec![],
                };
                table.canonicalize(def)?;
                next.masters.insert(
                    master_id,
                    MasterEntry {
                        data: Some(Master {
                            table,
                            comments: Comments::default(),
                        }),
                        error: None,
                    },
                );
            }
            Operation::ConfigureMaster {
                master_id,
                path,
                primary_key,
            } => {
                let (master, old) = get_master(&mut next, &master_id)?;
                if !master.table.rows.is_empty() {
                    return Err(
                        "Row が存在する Master の path / Primary Key は変更できません。".into(),
                    );
                }
                let old_path = old.path.clone();
                next.config
                    .masters
                    .insert(master_id.clone(), MasterDefinition { path, primary_key });
                next.config.validate()?;
                let def = &next.config.masters[&master_id];
                if def.path != old_path && storage::safe_path(&self.root, &def.path)?.exists() {
                    return Err("保存先に既存ファイルがあります。".into());
                }
                next.masters
                    .get_mut(&master_id)
                    .unwrap()
                    .data
                    .as_mut()
                    .unwrap()
                    .table
                    .canonicalize(def)?;
            }
            Operation::EditCells { master_id, edits } => {
                let (master, def) = get_master(&mut next, &master_id)?;
                // Validate all targets before applying any cell in the logical operation.
                let mut targets = vec![];
                for edit in edits {
                    if def.primary_key.contains(&edit.column) {
                        return Err("Primary Key を含む編集は操作全体を適用できません。".into());
                    }
                    let row = master.table.row_index(&edit.primary_key, def)?;
                    let col = column_index(&master.table, &edit.column)?;
                    targets.push((row, col, edit.value));
                }
                for (row, col, value) in targets {
                    master.table.rows[row][col] = value;
                }
            }
            Operation::CreateRows { master_id, rows } => {
                let (master, _) = get_master(&mut next, &master_id)?;
                master.table.rows.extend(rows);
            }
            Operation::AddRow {
                master_id,
                primary_key,
                duplicate_from,
            } => {
                let (master, def) = get_master(&mut next, &master_id)?;
                if primary_key.len() != def.primary_key.len()
                    || primary_key.iter().any(String::is_empty)
                {
                    return Err("すべての Primary Key component を入力してください。".into());
                }
                let mut row = if let Some(key) = duplicate_from {
                    master.table.rows[master.table.row_index(&key, def)?].clone()
                } else {
                    vec![String::new(); master.table.columns.len()]
                };
                for (i, value) in master.table.key_indices(def)?.into_iter().zip(primary_key) {
                    row[i] = value;
                }
                master.table.rows.push(row);
            }
            Operation::DeleteRows {
                master_id,
                primary_keys,
            } => {
                let (master, def) = get_master(&mut next, &master_id)?;
                for key in &primary_keys {
                    master.table.row_index(key, def)?;
                }
                let indices = master.table.key_indices(def)?;
                master
                    .table
                    .rows
                    .retain(|r| !primary_keys.contains(&Table::key(r, &indices)));
                master.comments.delete_rows(&primary_keys);
            }
            Operation::AddColumn { master_id, name } => {
                validate_column(&name)?;
                let (master, _) = get_master(&mut next, &master_id)?;
                if master.table.columns.contains(&name) {
                    return Err("Column 名が重複しています。".into());
                }
                master.table.columns.push(name);
                for row in &mut master.table.rows {
                    row.push(String::new());
                }
            }
            Operation::DeleteColumn { master_id, name } => {
                let (master, def) = get_master(&mut next, &master_id)?;
                if def.primary_key.contains(&name) {
                    return Err("Primary Key Column は削除できません。".into());
                }
                let index = column_index(&master.table, &name)?;
                master.table.columns.remove(index);
                for row in &mut master.table.rows {
                    row.remove(index);
                }
                master.comments.cells.retain(|c| c.column != name);
            }
            Operation::SetComment {
                master_id,
                target,
                body,
            } => {
                let (master, def) = get_master(&mut next, &master_id)?;
                match &target {
                    CommentTarget::Table => (),
                    CommentTarget::Row { primary_key } => {
                        master.table.row_index(primary_key, def)?;
                    }
                    CommentTarget::Cell {
                        primary_key,
                        column,
                    } => {
                        master.table.row_index(primary_key, def)?;
                        column_index(&master.table, column)?;
                    }
                }
                master.comments.set(&target, &body, &self.identity);
            }
        }
        for (id, entry) in &mut next.masters {
            if let Some(master) = &mut entry.data {
                let def = &next.config.masters[id];
                master.table.canonicalize(def)?;
                master.comments.validate(&master.table, def)?;
            }
        }
        if next != self.data {
            self.persist(&next)?;
            self.undo.push(std::mem::replace(&mut self.data, next));
            if self.undo.len() > 100 {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.revision += 1;
        }
        Ok(self.snapshot())
    }

    pub fn undo(&mut self, revision: u64) -> Result<Snapshot> {
        self.writable(revision)?;
        if let Some(next) = self.undo.last().cloned() {
            self.persist(&next)?;
            self.undo.pop();
            self.redo.push(std::mem::replace(&mut self.data, next));
            self.revision += 1;
        }
        Ok(self.snapshot())
    }

    pub fn redo(&mut self, revision: u64) -> Result<Snapshot> {
        self.writable(revision)?;
        if let Some(next) = self.redo.last().cloned() {
            self.persist(&next)?;
            self.redo.pop();
            self.undo.push(std::mem::replace(&mut self.data, next));
            self.revision += 1;
        }
        Ok(self.snapshot())
    }

    fn persist(&self, next: &ProjectData) -> Result<()> {
        let old_files = files(&self.data)?;
        let next_files = files(next)?;
        let paths: BTreeSet<_> = old_files.keys().chain(next_files.keys()).collect();
        let mut changes = vec![];
        for relative in paths {
            if old_files.get(relative) != next_files.get(relative) {
                changes.push(FileChange {
                    path: storage::safe_path(&self.root, relative)?,
                    bytes: next_files.get(relative).cloned(),
                });
            }
        }
        storage::transaction(changes)
    }

    pub fn git_status(&self) -> Result<GitStatus> {
        let branch = git(&self.root, &["branch", "--show-current"])?;
        let upstream = git(&self.root, &["rev-parse", "--abbrev-ref", "@{upstream}"]).ok();
        let (ahead, behind) = upstream
            .as_ref()
            .and_then(|_| {
                git(
                    &self.root,
                    &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
                )
                .ok()
            })
            .and_then(|s| {
                let mut n = s.split_whitespace().filter_map(|v| v.parse().ok());
                Some((n.next()?, n.next()?))
            })
            .unwrap_or((0, 0));
        let tracked_dirty = !git(
            &self.root,
            &["status", "--porcelain", "--untracked-files=no"],
        )?
        .is_empty();
        let merge_in_progress =
            git(&self.root, &["rev-parse", "-q", "--verify", "MERGE_HEAD"]).is_ok();
        let remotes = lines(&git(&self.root, &["remote"])?);
        let branches = lines(&git(
            &self.root,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
        )?);
        Ok(GitStatus {
            protected: self
                .data
                .config
                .git
                .protected_branches
                .iter()
                .any(|p| glob_matches(p, &branch)),
            branch,
            upstream,
            ahead,
            behind,
            tracked_dirty,
            merge_in_progress,
            remotes,
            branches,
        })
    }

    pub fn fetch(&mut self) -> Result<Snapshot> {
        git(&self.root, &["fetch", "--all", "--prune"])?;
        Ok(self.snapshot())
    }

    pub fn update(&mut self) -> Result<Snapshot> {
        self.require_clean()?;
        self.fetch()?;
        let status = self.git_status()?;
        if status.upstream.is_none() {
            return Err("Current branch に upstream がありません。".into());
        }
        self.start_merge("@{upstream}", status.protected)
    }

    pub fn switch_branch(&mut self, branch: &str, create: bool) -> Result<Snapshot> {
        self.require_clean()?;
        validate_branch(branch)?;
        if create {
            git(&self.root, &["switch", "-c", branch])?;
        } else {
            git(&self.root, &["switch", branch])?;
        }
        self.reload()
    }

    pub fn commit(&mut self, message: &str, push: bool) -> Result<Snapshot> {
        if self.git_status()?.merge_in_progress {
            return Err("Conflict Resolver から Merge を完了してください。".into());
        }
        if self.git_status()?.protected {
            return Err("Protected Branch では Commit できません。".into());
        }
        if !self.identity.is_complete() {
            return Err("Commit には Git Identity を設定してください。".into());
        }
        let message = message.trim();
        if message.is_empty() {
            return Err("Commit message を入力してください。".into());
        }
        let managed = self.managed_paths()?;
        if managed.is_empty() {
            return Err("Commit 対象の変更がありません。".into());
        }
        let staged = git_bytes(&self.root, &["diff", "--cached", "--name-only", "-z"])?;
        let unmanaged: Vec<_> = staged
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .filter_map(|p| std::str::from_utf8(p).ok())
            .filter(|p| !managed.contains(*p))
            .collect();
        if !unmanaged.is_empty() {
            return Err(format!(
                "管理対象外の staged file があります。先に Index を整理してください: {}",
                unmanaged.join(", ")
            ));
        }
        let mut args = vec!["add", "-A", "--"];
        let owned: Vec<String> = managed.into_iter().collect();
        args.extend(owned.iter().map(String::as_str));
        git(&self.root, &args)?;
        if git(&self.root, &["diff", "--cached", "--quiet"]).is_ok() {
            return Err("Commit 対象の変更がありません。".into());
        }
        git(&self.root, &["commit", "-m", message])?;
        if push {
            self.push_only()?;
        }
        self.reload()
    }

    pub fn push(&mut self) -> Result<Snapshot> {
        self.push_only()?;
        Ok(self.snapshot())
    }

    fn push_only(&self) -> Result<()> {
        let status = self.git_status()?;
        if status.upstream.is_some() {
            git(&self.root, &["push"])?;
        } else if status.remotes.iter().any(|r| r == "origin") {
            git(&self.root, &["push", "-u", "origin", &status.branch])?;
        } else {
            return Err("Push 先の origin / upstream がありません。".into());
        }
        Ok(())
    }

    pub(crate) fn require_clean(&self) -> Result<()> {
        let s = self.git_status()?;
        if s.tracked_dirty || s.merge_in_progress {
            return Err(
                "Branch / Update の前に tracked Working Tree と Index を clean にしてください。"
                    .into(),
            );
        }
        Ok(())
    }

    pub(crate) fn reload(&mut self) -> Result<Snapshot> {
        let fresh = Self::open(&self.root)?;
        self.merge = fresh.merge;
        self.data = fresh.data;
        self.identity = fresh.identity;
        self.undo.clear();
        self.redo.clear();
        self.revision += 1;
        Ok(self.snapshot())
    }

    fn managed_paths(&self) -> Result<BTreeSet<String>> {
        let mut result: BTreeSet<String> = files(&self.data)?.into_keys().collect();
        if let Some(head) = self.head_data()? {
            result.extend(files(&head)?.into_keys());
        }
        Ok(result)
    }

    fn head_data(&self) -> Result<Option<ProjectData>> {
        let config_bytes = match git_bytes(&self.root, &["show", &format!("HEAD:{CONFIG_PATH}")]) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };
        let config = ProjectConfig::parse(&config_bytes)?;
        let mut masters = BTreeMap::new();
        for (id, def) in &config.masters {
            let load = || -> Result<Master> {
                let table = Table::parse(
                    &git_bytes(&self.root, &["show", &format!("HEAD:{}", def.path)])?,
                    def,
                )?;
                let comments = match git_bytes(
                    &self.root,
                    &["show", &format!("HEAD:gamemasterstudio/comments/{id}.json")],
                ) {
                    Ok(v) => serde_json::from_slice(&v).map_err(|e| e.to_string())?,
                    Err(_) => Comments::default(),
                };
                Ok(Master { table, comments })
            };
            masters.insert(
                id.clone(),
                match load() {
                    Ok(data) => MasterEntry {
                        data: Some(data),
                        error: None,
                    },
                    Err(error) => MasterEntry {
                        data: None,
                        error: Some(error),
                    },
                },
            );
        }
        Ok(Some(ProjectData { config, masters }))
    }

    pub fn semantic_diff(&self) -> Result<Vec<SemanticChange>> {
        diff_data(self.head_data()?.as_ref(), &self.data)
    }

    /// Read both sides and the diff together, without requiring an editable branch.
    pub fn change_review(&self, revision: u64) -> Result<ChangeReview> {
        if revision != self.revision {
            return Err("編集状態が更新されました。もう一度操作してください。".into());
        }
        let before = self.head_data()?;
        let changes = diff_data(before.as_ref(), &self.data)?;
        Ok(ChangeReview {
            before,
            after: self.data.clone(),
            changes,
        })
    }

    pub fn revert_change(&mut self, change: SemanticChange, revision: u64) -> Result<Snapshot> {
        self.writable(revision)?;
        let head = self.head_data()?.ok_or("HEAD に Project がありません。")?;
        let mut next = self.data.clone();
        apply_revert(&mut next, &head, &change)?;
        self.persist(&next)?;
        self.undo.push(std::mem::replace(&mut self.data, next));
        self.redo.clear();
        self.revision += 1;
        Ok(self.snapshot())
    }
}

fn lines(value: &str) -> Vec<String> {
    value
        .lines()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}
fn glob_matches(pattern: &str, value: &str) -> bool {
    glob::Pattern::new(pattern).is_ok_and(|p| {
        p.matches_with(
            value,
            glob::MatchOptions {
                case_sensitive: true,
                require_literal_separator: false,
                require_literal_leading_dot: false,
            },
        )
    })
}
fn validate_branch(branch: &str) -> Result<()> {
    if branch.trim() != branch
        || branch.is_empty()
        || branch.starts_with('-')
        || branch.contains(['\0', '\n', '\r', ' '])
    {
        Err("Branch 名が不正です。".into())
    } else {
        Ok(())
    }
}
pub(crate) fn git_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().into())
    }
}

pub(crate) fn diff_data(
    base: Option<&ProjectData>,
    work: &ProjectData,
) -> Result<Vec<SemanticChange>> {
    let mut out = vec![];
    let empty = ProjectData {
        config: ProjectConfig::default(),
        masters: BTreeMap::new(),
    };
    if base.map(|b| &b.config.git) != Some(&work.config.git) {
        out.push(SemanticChange::ProjectConfig {
            master_id: "(Project Settings)".into(),
            before: base.map(|b| b.config.git.clone()),
            after: work.config.git.clone(),
        });
    }
    let base = base.unwrap_or(&empty);
    for data in [base, work] {
        for (id, entry) in &data.masters {
            if let Some(error) = &entry.error {
                return Err(format!("{id}: {error}"));
            }
        }
    }
    let ids: BTreeSet<_> = base
        .config
        .masters
        .keys()
        .chain(work.config.masters.keys())
        .cloned()
        .collect();
    for id in ids {
        let b = base.masters.get(&id).and_then(|e| e.data.as_ref());
        let w = work.masters.get(&id).and_then(|e| e.data.as_ref());
        match (b, w) {
            (None, Some(_)) => {
                out.push(SemanticChange::AddedMaster { master_id: id });
                continue;
            }
            (Some(_), None) => {
                out.push(SemanticChange::DeletedMaster { master_id: id });
                continue;
            }
            (Some(b), Some(w)) => diff_master(
                &id,
                b,
                &base.config.masters[&id],
                w,
                &work.config.masters[&id],
                &mut out,
            )?,
            _ => (),
        }
    }
    Ok(out)
}

fn diff_master(
    id: &str,
    b: &Master,
    bd: &MasterDefinition,
    w: &Master,
    wd: &MasterDefinition,
    out: &mut Vec<SemanticChange>,
) -> Result<()> {
    if bd != wd {
        out.push(SemanticChange::MasterDefinition {
            master_id: id.into(),
            before: bd.clone(),
            after: wd.clone(),
        });
        // No row identity mapping across different primary key definitions.
        if bd.primary_key != wd.primary_key {
            return Ok(());
        }
    }
    for c in &w.table.columns {
        if !b.table.columns.contains(c) {
            out.push(SemanticChange::AddedColumn {
                master_id: id.into(),
                column: c.clone(),
            });
        }
    }
    for c in &b.table.columns {
        if !w.table.columns.contains(c) {
            out.push(SemanticChange::DeletedColumn {
                master_id: id.into(),
                column: c.clone(),
            });
        }
    }
    let rows = |m: &Master,
                d: &MasterDefinition|
     -> Result<BTreeMap<PrimaryKey, BTreeMap<String, String>>> {
        let ki = m.table.key_indices(d)?;
        Ok(m.table
            .rows
            .iter()
            .map(|r| {
                (
                    Table::key(r, &ki),
                    m.table
                        .columns
                        .iter()
                        .cloned()
                        .zip(r.iter().cloned())
                        .collect(),
                )
            })
            .collect())
    };
    let (br, wr) = (rows(b, bd)?, rows(w, wd)?);
    let keys: BTreeSet<_> = br.keys().chain(wr.keys()).cloned().collect();
    for key in keys {
        match (br.get(&key), wr.get(&key)) {
            (None, Some(_)) => out.push(SemanticChange::AddedRow {
                master_id: id.into(),
                primary_key: key,
            }),
            (Some(_), None) => out.push(SemanticChange::DeletedRow {
                master_id: id.into(),
                primary_key: key,
            }),
            (Some(a), Some(z)) => {
                for c in b
                    .table
                    .columns
                    .iter()
                    .filter(|c| w.table.columns.contains(c))
                {
                    if a.get(c) != z.get(c) {
                        out.push(SemanticChange::Cell {
                            master_id: id.into(),
                            primary_key: key.clone(),
                            column: c.clone(),
                            before: a[c].clone(),
                            after: z[c].clone(),
                        });
                    }
                }
            }
            _ => (),
        }
    }
    let comments = |m: &Master| {
        let mut x: BTreeMap<String, (CommentTarget, String)> = BTreeMap::new();
        if let Some(c) = &m.comments.table {
            x.insert("t".into(), (CommentTarget::Table, c.body.clone()));
        }
        for c in &m.comments.rows {
            x.insert(
                format!("r{}", serde_json::to_string(&c.primary_key).unwrap()),
                (
                    CommentTarget::Row {
                        primary_key: c.primary_key.clone(),
                    },
                    c.comment.body.clone(),
                ),
            );
        }
        for c in &m.comments.cells {
            x.insert(
                format!(
                    "c{}:{}",
                    serde_json::to_string(&c.primary_key).unwrap(),
                    c.column
                ),
                (
                    CommentTarget::Cell {
                        primary_key: c.primary_key.clone(),
                        column: c.column.clone(),
                    },
                    c.comment.body.clone(),
                ),
            );
        }
        x
    };
    let (bc, wc) = (comments(b), comments(w));
    let targets: BTreeSet<_> = bc.keys().chain(wc.keys()).cloned().collect();
    for k in targets {
        let before = bc.get(&k).map(|x| x.1.clone());
        let after = wc.get(&k).map(|x| x.1.clone());
        if before != after {
            let target = wc.get(&k).or_else(|| bc.get(&k)).unwrap().0.clone();
            out.push(SemanticChange::Comment {
                master_id: id.into(),
                target,
                before,
                after,
            });
        }
    }
    Ok(())
}

fn apply_revert(next: &mut ProjectData, head: &ProjectData, change: &SemanticChange) -> Result<()> {
    match change {
        SemanticChange::ProjectConfig { .. } => next.config.git = head.config.git.clone(),
        SemanticChange::MasterDefinition { master_id, .. } => {
            next.config.masters.insert(
                master_id.clone(),
                head.config
                    .masters
                    .get(master_id)
                    .ok_or("HEAD に Master 定義がありません。")?
                    .clone(),
            );
            next.masters.insert(
                master_id.clone(),
                head.masters
                    .get(master_id)
                    .ok_or("HEAD に Master がありません。")?
                    .clone(),
            );
        }
        SemanticChange::AddedMaster { master_id } => {
            next.masters.remove(master_id);
            next.config.masters.remove(master_id);
        }
        SemanticChange::DeletedMaster { master_id } => {
            next.config
                .masters
                .insert(master_id.clone(), head.config.masters[master_id].clone());
            next.masters
                .insert(master_id.clone(), head.masters[master_id].clone());
        }
        SemanticChange::AddedColumn { master_id, column } => {
            let (m, d) = get_master(next, master_id)?;
            if d.primary_key.contains(column) {
                return Err("Primary Key Column は revert できません。".into());
            }
            let i = column_index(&m.table, column)?;
            m.table.columns.remove(i);
            for r in &mut m.table.rows {
                r.remove(i);
            }
            m.comments.cells.retain(|c| c.column != *column);
        }
        SemanticChange::DeletedColumn { master_id, column } => {
            let hm = head.masters[master_id]
                .data
                .as_ref()
                .ok_or("HEAD Master が不正です。")?;
            let hd = &head.config.masters[master_id];
            let source = hm
                .table
                .columns
                .iter()
                .position(|c| c == column)
                .ok_or("HEAD Column がありません。")?;
            let head_keys = hm.table.key_indices(hd)?;
            let values: BTreeMap<_, _> = hm
                .table
                .rows
                .iter()
                .map(|r| (Table::key(r, &head_keys), r[source].clone()))
                .collect();
            let (m, d) = get_master(next, master_id)?;
            m.table.columns.push(column.clone());
            let keys = m.table.key_indices(d)?;
            for r in &mut m.table.rows {
                r.push(
                    values
                        .get(&Table::key(r, &keys))
                        .cloned()
                        .unwrap_or_default(),
                );
            }
        }
        SemanticChange::AddedRow {
            master_id,
            primary_key,
        } => {
            let (m, d) = get_master(next, master_id)?;
            let i = m.table.row_index(primary_key, d)?;
            m.table.rows.remove(i);
            m.comments.delete_rows(std::slice::from_ref(primary_key));
        }
        SemanticChange::DeletedRow {
            master_id,
            primary_key,
        } => {
            let hm = head.masters[master_id]
                .data
                .as_ref()
                .ok_or("HEAD Master が不正です。")?;
            let hd = &head.config.masters[master_id];
            let original = &hm.table.rows[hm.table.row_index(primary_key, hd)?];
            let (m, _) = get_master(next, master_id)?;
            // Other column changes may still be pending when a row is restored.
            let row = m
                .table
                .columns
                .iter()
                .map(|column| {
                    hm.table
                        .columns
                        .iter()
                        .position(|c| c == column)
                        .map(|i| original[i].clone())
                        .unwrap_or_default()
                })
                .collect();
            m.table.rows.push(row);
        }
        SemanticChange::Cell {
            master_id,
            primary_key,
            column,
            before,
            ..
        } => {
            let (m, d) = get_master(next, master_id)?;
            let r = m.table.row_index(primary_key, d)?;
            let c = column_index(&m.table, column)?;
            m.table.rows[r][c] = before.clone();
        }
        SemanticChange::Comment {
            master_id, target, ..
        } => {
            let original = head.masters[master_id]
                .data
                .as_ref()
                .ok_or("HEAD Master が不正です。")?
                .comments
                .clone();
            let (m, _) = get_master(next, master_id)?;
            match target {
                CommentTarget::Table => m.comments.table = original.table,
                CommentTarget::Row { primary_key } => {
                    m.comments.rows.retain(|c| c.primary_key != *primary_key);
                    if let Some(c) = original
                        .rows
                        .into_iter()
                        .find(|c| c.primary_key == *primary_key)
                    {
                        m.comments.rows.push(c);
                    }
                }
                CommentTarget::Cell {
                    primary_key,
                    column,
                } => {
                    m.comments
                        .cells
                        .retain(|c| c.primary_key != *primary_key || c.column != *column);
                    if let Some(c) = original
                        .cells
                        .into_iter()
                        .find(|c| c.primary_key == *primary_key && c.column == *column)
                    {
                        m.comments.cells.push(c);
                    }
                }
            }
        }
    }
    for (id, e) in &mut next.masters {
        if let Some(m) = &mut e.data {
            let d = &next.config.masters[id];
            m.table.canonicalize(d)?;
            m.comments.validate(&m.table, d)?;
        }
    }
    Ok(())
}

fn get_master<'a>(
    data: &'a mut ProjectData,
    id: &str,
) -> Result<(&'a mut Master, &'a MasterDefinition)> {
    let def = data.config.masters.get(id).ok_or("Master がありません。")?;
    let entry = data.masters.get_mut(id).ok_or("Master がありません。")?;
    let master = entry.data.as_mut().ok_or_else(|| {
        entry
            .error
            .clone()
            .unwrap_or_else(|| "Master を読み込めません。".into())
    })?;
    Ok((master, def))
}

fn column_index(table: &Table, column: &str) -> Result<usize> {
    table
        .columns
        .iter()
        .position(|c| c == column)
        .ok_or_else(|| format!("Column がありません: {column}"))
}

fn files(data: &ProjectData) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::from([(CONFIG_PATH.into(), data.config.serialize()?)]);
    for (id, entry) in &data.masters {
        if let Some(master) = &entry.data {
            let def = &data.config.masters[id];
            files.insert(def.path.clone(), master.table.serialize(def)?);
            if let Some(bytes) = master.comments.serialize()? {
                files.insert(format!("gamemasterstudio/comments/{id}.json"), bytes);
            }
        }
    }
    Ok(files)
}
