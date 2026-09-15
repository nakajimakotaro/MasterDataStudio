//! Read-only history reconstructed from Git snapshots. No separate history store.
use crate::{
    comments::Identity,
    merge_git::read_source,
    project::{diff_data, git, git_bytes, Project, SemanticChange},
    Result,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const PAGE_SIZE: usize = 50;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCommit {
    pub oid: String,
    pub parents: Vec<String>,
    pub author: Identity,
    pub authored_at: String,
    pub subject: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub head: Option<String>,
    pub commits: Vec<HistoryCommit>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDetail {
    pub oid: String,
    pub parent: Option<String>,
    pub message: String,
    pub changes: Vec<SemanticChange>,
}
fn commit_oid(root: &std::path::Path, oid: &str) -> Result<String> {
    if ![40, 64].contains(&oid.len()) || !oid.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Commit ID が不正です。".into());
    }
    git(
        root,
        &["rev-parse", "--verify", &format!("{oid}^{{commit}}")],
    )
}
impl Project {
    /// First-parent history keeps a merge as one entry; details compare its first parent.
    pub fn history(&self, head: Option<&str>, offset: usize) -> Result<HistoryPage> {
        self.history_query(head, offset, "", "")
    }
    /// A cursor is the first unreturned commit, so later pages never rescan earlier history.
    pub fn history_search(
        &self,
        cursor: Option<&str>,
        query: &str,
        author: &str,
    ) -> Result<HistoryPage> {
        self.history_query(cursor, 0, query, author)
    }
    fn history_query(
        &self,
        head: Option<&str>,
        offset: usize,
        query: &str,
        author: &str,
    ) -> Result<HistoryPage> {
        let head = match head {
            Some(oid) => commit_oid(&self.root, oid)?,
            None => match git(&self.root, &["rev-parse", "--verify", "HEAD"]) {
                Ok(oid) => oid,
                Err(_) => {
                    return Ok(HistoryPage {
                        head: None,
                        commits: vec![],
                        has_more: false,
                        next_cursor: None,
                    })
                }
            },
        };
        let bytes = git_bytes(
            &self.root,
            &[
                "log",
                "--first-parent",
                "--fixed-strings",
                "--regexp-ignore-case",
                &format!("--grep={query}"),
                &format!("--author={author}"),
                "-z",
                "--encoding=UTF-8",
                "--format=%H%x00%P%x00%an%x00%ae%x00%aI%x00%s",
                &format!("--skip={offset}"),
                &format!("--max-count={}", PAGE_SIZE + 1),
                &head,
                "--",
            ],
        )?;
        let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        let fields: Vec<_> = text
            .strip_suffix('\0')
            .unwrap_or(&text)
            .split('\0')
            .collect();
        let mut commits = vec![];
        if !text.is_empty() {
            if fields.len() % 6 != 0 {
                return Err("Git History の形式が不正です。".into());
            }
            for f in fields.chunks_exact(6) {
                commits.push(HistoryCommit {
                    oid: f[0].into(),
                    parents: f[1].split_whitespace().map(str::to_owned).collect(),
                    author: Identity {
                        name: f[2].into(),
                        email: f[3].into(),
                    },
                    authored_at: f[4].into(),
                    subject: f[5].into(),
                });
            }
        }
        let has_more = commits.len() > PAGE_SIZE;
        let next_cursor = commits.get(PAGE_SIZE).map(|c| c.oid.clone());
        commits.truncate(PAGE_SIZE);
        Ok(HistoryPage {
            head: Some(head),
            commits,
            has_more,
            next_cursor,
        })
    }
    pub fn history_detail(&self, oid: &str) -> Result<HistoryDetail> {
        let oid = commit_oid(&self.root, oid)?;
        let parents = git(&self.root, &["show", "-s", "--format=%P", &oid, "--"])?;
        let parent = parents.split_whitespace().next().map(str::to_owned);
        let work = read_source(&self.root, &oid)?;
        let base = parent
            .as_deref()
            .map(|p| read_source(&self.root, p))
            .transpose()?;
        let has_config = |revision: &str| {
            git(
                &self.root,
                &[
                    "cat-file",
                    "-e",
                    &format!("{revision}:{}", crate::config::CONFIG_PATH),
                ],
            )
            .is_ok()
        };
        let base = base.filter(|_| parent.as_deref().is_some_and(has_config));
        let changes = if base.is_none() && !has_config(&oid) {
            vec![]
        } else {
            diff_data(base.as_ref(), &work)?
        };
        let message = git(
            &self.root,
            &["show", "-s", "--encoding=UTF-8", "--format=%B", &oid, "--"],
        )?;
        Ok(HistoryDetail {
            oid,
            parent,
            message,
            changes,
        })
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeFilter {
    #[serde(default)]
    pub master: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub column: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub offset: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryChangesPage {
    pub oid: String,
    pub message: String,
    pub total: usize,
    pub matched: usize,
    pub masters: BTreeMap<String, usize>,
    pub kinds: BTreeMap<String, usize>,
    pub columns: BTreeMap<String, usize>,
    pub changes: Vec<SemanticChange>,
    pub offset: usize,
    pub page_size: usize,
}
impl HistoryDetail {
    /// Only the requested page crosses IPC. The desktop adapter retains one detail at a time.
    pub fn page(&self, filter: &ChangeFilter) -> HistoryChangesPage {
        let mut masters = BTreeMap::new();
        let mut kinds = BTreeMap::new();
        let mut columns = BTreeMap::new();
        let mut matched = 0;
        let mut changes = Vec::new();
        let query = filter.query.to_lowercase();
        for change in &self.changes {
            let (master, kind, column) = change_location(change);
            *masters.entry(master.to_owned()).or_insert(0) += 1;
            if !filter.master.is_empty() && filter.master != master {
                continue;
            }
            *kinds.entry(kind.to_owned()).or_insert(0) += 1;
            if !column.is_empty() {
                *columns.entry(column.to_owned()).or_insert(0) += 1;
            }
            if (!filter.kind.is_empty() && filter.kind != kind)
                || (!filter.column.is_empty() && filter.column != column)
                || (!query.is_empty()
                    && !serde_json::to_string(change)
                        .expect("semantic change serializes")
                        .to_lowercase()
                        .contains(&query))
            {
                continue;
            }
            if matched >= filter.offset && changes.len() < 100 {
                changes.push(change.clone());
            }
            matched += 1;
        }
        HistoryChangesPage {
            oid: self.oid.clone(),
            message: self.message.clone(),
            total: self.changes.len(),
            matched,
            masters,
            kinds,
            columns,
            changes,
            offset: filter.offset,
            page_size: 100,
        }
    }
}

fn change_location(change: &SemanticChange) -> (&str, &str, &str) {
    use SemanticChange::*;
    match change {
        ProjectConfig { master_id, .. } => (master_id, "projectConfig", ""),
        MasterDefinition { master_id, .. } => (master_id, "masterDefinition", ""),
        AddedMaster { master_id } => (master_id, "addedMaster", ""),
        DeletedMaster { master_id } => (master_id, "deletedMaster", ""),
        AddedColumn { master_id, column } => (master_id, "addedColumn", column),
        DeletedColumn { master_id, column } => (master_id, "deletedColumn", column),
        AddedRow { master_id, .. } => (master_id, "addedRow", ""),
        DeletedRow { master_id, .. } => (master_id, "deletedRow", ""),
        Cell {
            master_id, column, ..
        } => (master_id, "cell", column),
        Comment {
            master_id, target, ..
        } => (
            master_id,
            "comment",
            match target {
                crate::comments::CommentTarget::Cell { column, .. } => column,
                _ => "",
            },
        ),
    }
}
