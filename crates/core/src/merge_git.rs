//! Git adapter. Unmerged paths are read exclusively from index stages, never the working tree.
use crate::{
    comments::{Comment, Comments, Identity},
    config::{ProjectConfig, CONFIG_PATH},
    csv_data::Table,
    merge::{merge_project, MergePlan, MergeView, Resolution},
    project::{git, git_bytes, Master, MasterEntry, Project, ProjectData, Snapshot},
    storage::{self, FileChange},
    Result,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

type Stages = BTreeMap<String, BTreeMap<u8, String>>;

fn index_stages(root: &Path) -> Result<Stages> {
    let bytes = git_bytes(root, &["ls-files", "--unmerged", "-z"])?;
    let mut paths = Stages::new();
    for record in bytes.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let record = std::str::from_utf8(record).map_err(|e| e.to_string())?;
        let (meta, path) = record
            .split_once('\t')
            .ok_or("Git index record が不正です。")?;
        let mut fields = meta.split_whitespace();
        let mode = fields.next().ok_or("Git index mode がありません。")?;
        if mode != "100644" && mode != "100755" {
            return Err(format!("通常ファイル以外の競合は解決できません: {path}"));
        }
        let oid = fields.next().ok_or("Git index object がありません。")?;
        let stage = fields
            .next()
            .ok_or("Git index stage がありません。")?
            .parse::<u8>()
            .map_err(|e| e.to_string())?;
        paths
            .entry(path.into())
            .or_default()
            .insert(stage, oid.into());
    }
    Ok(paths)
}

fn blob(root: &Path, revision: &str, path: &str) -> Result<Option<Vec<u8>>> {
    let spec = format!("{revision}:{path}");
    if git(root, &["cat-file", "-e", &spec]).is_err() {
        return Ok(None);
    }
    git_bytes(root, &["show", &spec]).map(Some)
}
fn source_blob(
    root: &Path,
    stages: &Stages,
    side: u8,
    revision: &str,
    path: &str,
) -> Result<Option<Vec<u8>>> {
    if let Some(stages) = stages.get(path) {
        // Missing stages are ABSENT (add/add and modify/delete).
        stages
            .get(&side)
            .map(|oid| git_bytes(root, &["cat-file", "blob", oid]))
            .transpose()
    } else {
        blob(root, revision, path)
    }
}

pub(crate) fn read_source(root: &Path, revision: &str) -> Result<ProjectData> {
    read_side(root, &Stages::new(), 0, revision)
}
fn read_side(root: &Path, stages: &Stages, side: u8, revision: &str) -> Result<ProjectData> {
    let config = source_blob(root, stages, side, revision, CONFIG_PATH)?
        .map(|b| ProjectConfig::parse(&b))
        .transpose()?
        .unwrap_or_default();
    let mut masters = BTreeMap::new();
    for (id, def) in &config.masters {
        let data = (|| -> Result<Master> {
            let bytes = source_blob(root, stages, side, revision, &def.path)?
                .ok_or_else(|| format!("{id}: {} が stage {side} にありません。", def.path))?;
            let table = Table::parse(&bytes, def)?;
            let mut comments: Comments =
                source_blob(root, stages, side, revision, &comment_path(id))?
                    .map(|b| {
                        serde_json::from_slice(&b).map_err(|e| format!("{id}: Comment JSON: {e}"))
                    })
                    .transpose()?
                    .unwrap_or_default();
            comments.validate(&table, def)?;
            let mut scripts: crate::scripts::Scripts =
                source_blob(root, stages, side, revision, &script_path(id))?
                    .map(|b| {
                        serde_json::from_slice(&b).map_err(|e| format!("{id}: Script JSON: {e}"))
                    })
                    .transpose()?
                    .unwrap_or_default();
            scripts.validate(&table, def)?;
            Ok(Master {
                table,
                comments,
                scripts,
                script_error: None,
            })
        })();
        masters.insert(
            id.clone(),
            match data {
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
    Ok(ProjectData { config, masters })
}
fn script_path(id: &str) -> String {
    format!("gamemasterstudio/scripts/{id}.json")
}
fn comment_path(id: &str) -> String {
    format!("gamemasterstudio/comments/{id}.json")
}
fn paths(data: &ProjectData) -> BTreeSet<String> {
    let mut out = BTreeSet::from([CONFIG_PATH.into()]);
    for (id, def) in &data.config.masters {
        out.insert(def.path.clone());
        out.insert(comment_path(id));
        out.insert(script_path(id));
    }
    out
}

pub(crate) struct MergeSession {
    sources: Option<[ProjectData; 3]>,
    choices: BTreeMap<String, Resolution>,
    pub(crate) plan: Option<MergePlan>,
    pub(crate) error: Option<String>,
}
impl MergeSession {
    pub(crate) fn recover(root: &Path) -> Self {
        match Self::load(root) {
            Ok(session) => session,
            Err(error) => Self {
                sources: None,
                choices: BTreeMap::new(),
                plan: None,
                error: Some(error),
            },
        }
    }
    fn load(root: &Path) -> Result<Self> {
        let stages = index_stages(root)?;
        if let Some(path) = stages
            .keys()
            .find(|p| p.starts_with("gamemasterstudio/scripts/"))
        {
            return Err(format!(
                "Script metadata conflict は自動解決できません: {path}"
            ));
        }
        let bases = git(root, &["merge-base", "--all", "HEAD", "MERGE_HEAD"])?;
        let bases: Vec<_> = bases.lines().collect();
        if bases.len() != 1 {
            return Err(
                "複数の merge base を持つ Merge は対応していません。Merge を中止してください。"
                    .into(),
            );
        }
        let sources = [
            read_side(root, &stages, 1, bases[0])?,
            read_side(root, &stages, 2, "HEAD")?,
            read_side(root, &stages, 3, "MERGE_HEAD")?,
        ];
        let mut managed = BTreeSet::new();
        for source in &sources {
            managed.extend(paths(source));
        }
        // Include all versioned metadata paths, including files of deleted Masters.
        for revision in [bases[0], "HEAD", "MERGE_HEAD"] {
            let files = git_bytes(
                root,
                &[
                    "ls-tree",
                    "-r",
                    "--name-only",
                    "-z",
                    revision,
                    "--",
                    "gamemasterstudio/comments/",
                ],
            )?;
            for path in files.split(|b| *b == 0).filter(|p| !p.is_empty()) {
                managed.insert(std::str::from_utf8(path).map_err(|e| e.to_string())?.into());
            }
        }
        let unsupported: Vec<_> = stages
            .keys()
            .filter(|p| !managed.contains(*p))
            .cloned()
            .collect();
        if !unsupported.is_empty() {
            return Err(format!("管理対象外の競合: {}", unsupported.join(", ")));
        }
        // Unknown metadata cannot be silently discarded during canonical output.
        for path in stages
            .keys()
            .filter(|p| p.starts_with("gamemasterstudio/comments/"))
        {
            if !sources
                .iter()
                .any(|s| s.config.masters.keys().any(|id| comment_path(id) == *path))
            {
                return Err(format!(
                    "Master 定義のないコメント競合は解決できません: {path}"
                ));
            }
        }
        let choices = BTreeMap::new();
        let plan = merge_project(&sources[0], &sources[1], &sources[2], &choices)?;
        Ok(Self {
            sources: Some(sources),
            choices,
            plan: Some(plan),
            error: None,
        })
    }
    pub(crate) fn view(&self) -> MergeView {
        self.plan
            .as_ref()
            .map(|p| p.view.clone())
            .unwrap_or_else(|| MergeView {
                error: self.error.clone(),
                ..MergeView::default()
            })
    }
    fn resolve_many(
        &mut self,
        ids: Vec<String>,
        mut resolution: Resolution,
        identity: &Identity,
    ) -> Result<()> {
        if ids.is_empty() {
            return Err("解決対象を選択してください。".into());
        }
        if ids.len() > 1 && !matches!(resolution, Resolution::Ours | Resolution::Theirs) {
            return Err("一括解決では Your Branch / Incoming を選択してください。".into());
        }
        let conflicts: BTreeMap<_, _> = self
            .plan
            .as_ref()
            .ok_or("Merge input がありません。")?
            .view
            .conflicts
            .iter()
            .map(|c| (c.id.as_str(), c))
            .collect();
        // Validate the entire set before changing any choices; stale IDs never partially apply.
        for id in &ids {
            if !conflicts.contains_key(id.as_str()) {
                return Err("Conflict がありません。最新の状態でやり直してください。".into());
            }
        }
        let conflict = conflicts[ids[0].as_str()];
        if let Resolution::Custom(body) = &resolution {
            if conflict.kind == "comment" {
                let body = crate::lf(body);
                let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
                let old: Option<Comment> = [&conflict.base, &conflict.ours, &conflict.theirs]
                    .into_iter()
                    .find_map(|v| {
                        serde_json::from_value::<Option<Comment>>(v.clone())
                            .ok()
                            .flatten()
                    });
                resolution = Resolution::Comment((!body.trim().is_empty()).then(|| {
                    Comment {
                        body,
                        created_by: old
                            .as_ref()
                            .map(|c| c.created_by.clone())
                            .unwrap_or_else(|| identity.clone()),
                        created_at: old
                            .as_ref()
                            .map(|c| c.created_at.clone())
                            .unwrap_or_else(|| now.clone()),
                        updated_by: identity.clone(),
                        updated_at: now,
                    }
                }));
            } else if conflict.kind != "cell" {
                return Err("Custom 値を指定できるのは Cell / Comment Conflict だけです。".into());
            }
        }
        let mut choices = self.choices.clone();
        for id in ids {
            choices.insert(id, resolution.clone());
        }
        let sources = self.sources.as_ref().ok_or("Merge input がありません。")?;
        let plan = merge_project(&sources[0], &sources[1], &sources[2], &choices)?;
        self.choices = choices;
        self.plan = Some(plan);
        Ok(())
    }
}

impl Project {
    pub fn merge_branch(&mut self, branch: &str) -> Result<Snapshot> {
        self.require_clean()?;
        if self.git_status()?.protected {
            return Err("Protected Branch では手動 Merge できません。".into());
        }
        let revision = git(
            &self.root,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{branch}^{{commit}}"),
            ],
        )?;
        self.start_merge(&revision, false)
    }
    pub(crate) fn start_merge(&mut self, revision: &str, ff_only: bool) -> Result<Snapshot> {
        if !ff_only && !self.identity.is_complete() {
            return Err("Merge には Git Identity を設定してください。".into());
        }
        // --no-commit also lets us inspect merges that Git textually auto-resolved.
        let args = if ff_only {
            vec!["merge", "--ff-only", revision]
        } else {
            vec!["merge", "--no-commit", "--no-edit", revision]
        };
        let result = git(&self.root, &args);
        if git(&self.root, &["rev-parse", "--verify", "MERGE_HEAD"]).is_ok() {
            let session = MergeSession::recover(&self.root);
            if let Some(error) = &session.error {
                // All app-initiated merges start clean, so abort restores their input.
                let error = error.clone();
                git(&self.root, &["merge", "--abort"])
                    .map_err(|abort| format!("{error}。Merge 中止にも失敗しました: {abort}"))?;
                self.reload()?;
                return Err(format!("Merge を中止し、開始前の状態へ戻しました: {error}"));
            }
            self.merge = Some(session);
            self.undo.clear();
            self.redo.clear();
            self.revision += 1;
            if self
                .merge
                .as_ref()
                .is_some_and(|m| m.view().remaining == 0 && m.view().error.is_none())
            {
                return self.complete_merge("", self.revision);
            }
            return Ok(self.snapshot());
        }
        result?;
        self.reload()
    }
    pub fn resolve_conflict(
        &mut self,
        id: String,
        resolution: Resolution,
        revision: u64,
    ) -> Result<Snapshot> {
        self.resolve_conflicts(vec![id], resolution, revision)
    }
    pub fn resolve_conflicts(
        &mut self,
        ids: Vec<String>,
        resolution: Resolution,
        revision: u64,
    ) -> Result<Snapshot> {
        self.merge_writable(revision)?;
        self.merge
            .as_mut()
            .ok_or("Merge 中ではありません。")?
            .resolve_many(ids, resolution, &self.identity)?;
        self.revision += 1;
        Ok(self.snapshot())
    }
    fn merge_writable(&self, revision: u64) -> Result<()> {
        if revision != self.revision {
            return Err("Merge 状態が更新されました。もう一度操作してください。".into());
        }
        let status = self.git_status()?;
        if !status.merge_in_progress || self.merge.is_none() {
            return Err("Merge 中ではありません。".into());
        }
        if status.protected {
            return Err(
                "Protected Branch では Merge commit を作成できません。Merge を中止してください。"
                    .into(),
            );
        }
        if !self.identity.is_complete() {
            return Err("Git Identity を設定してください。".into());
        }
        Ok(())
    }
    pub fn complete_merge(&mut self, message: &str, revision: u64) -> Result<Snapshot> {
        self.merge_writable(revision)?;
        let session = self.merge.as_ref().unwrap();
        let plan = session
            .plan
            .as_ref()
            .ok_or("Merge input を読み込めません。")?;
        if plan.view.remaining > 0 {
            return Err("すべての Conflict を解決してください。".into());
        }
        if let Some(error) = &plan.view.error {
            return Err(error.clone());
        }
        let sources = session.sources.as_ref().unwrap();
        let mut outputs =
            BTreeMap::from([(CONFIG_PATH.to_string(), Some(plan.data.config.serialize()?))]);
        for path in sources.iter().flat_map(paths) {
            outputs.entry(path).or_insert(None);
        }
        for (id, def) in &plan.data.config.masters {
            let master = plan.data.masters[id]
                .data
                .as_ref()
                .ok_or("Master がありません。")?;
            outputs.insert(def.path.clone(), Some(master.table.serialize(def)?));
            outputs.insert(comment_path(id), master.comments.serialize()?);
            outputs.insert(script_path(id), master.scripts.serialize()?);
        }
        let mut changes = vec![];
        let mut stage = BTreeSet::new();
        for (path, bytes) in outputs {
            let target = storage::safe_path(&self.root, &path)?;
            // Do not overwrite untracked files when choosing a path from another definition.
            let tracked = !git_bytes(
                &self.root,
                &["--literal-pathspecs", "ls-files", "-z", "--", &path],
            )?
            .is_empty();
            if !tracked && target.exists() {
                return Err(format!("未追跡ファイルが保存先にあります: {path}"));
            }
            if tracked || bytes.is_some() {
                stage.insert(path);
            }
            changes.push(FileChange {
                path: target,
                bytes,
            });
        }
        // The clean pre-merge index guarantees that nonconflicting unmanaged
        // paths currently staged by Git belong to this merge. Leave them intact.
        let index_path = git(
            &self.root,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        )?;
        let index_backup = fs::read(&index_path).map_err(|e| e.to_string())?;
        let rollback = changes
            .iter()
            .map(|change| {
                Ok(FileChange {
                    path: change.path.clone(),
                    bytes: storage::read_optional(&change.path)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        storage::transaction(changes)?;
        let mut args = vec!["--literal-pathspecs", "add", "-A", "--"];
        args.extend(stage.iter().map(String::as_str));
        let result = (|| -> Result<()> {
            git(&self.root, &args)?;
            if !index_stages(&self.root)?.is_empty() {
                return Err("未解決の Git stage が残っています。".into());
            }
            if message.trim().is_empty() {
                git(&self.root, &["commit", "--no-edit"])?;
            } else {
                git(&self.root, &["commit", "-m", message.trim()])?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            let file_restore = storage::transaction(rollback);
            let index_restore = storage::atomic_write(Path::new(&index_path), &index_backup);
            if file_restore.is_err() || index_restore.is_err() {
                return Err(format!(
                    "{error}。復元失敗: files={file_restore:?}, index={index_restore:?}"
                ));
            }
            return Err(format!(
                "Merge 完了に失敗しました。解決内容を保持しています。再実行できます: {error}"
            ));
        }
        self.reload()
    }
    pub fn abort_merge(&mut self) -> Result<Snapshot> {
        if !self.git_status()?.merge_in_progress {
            return Err("Merge 中ではありません。".into());
        }
        git(&self.root, &["merge", "--abort"])?;
        self.reload()
    }
}
