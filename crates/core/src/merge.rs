//! Pure semantic three-way merge. Option::None is ABSENT, never an empty cell.
use crate::{
    comments::{CellComment, Comment, Comments, RowComment},
    config::{MasterDefinition, ProjectConfig},
    csv_data::{PrimaryKey, Table},
    project::{Master, MasterEntry, ProjectData},
    Result,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum Resolution {
    Ours,
    Theirs,
    Custom(String),
    // Created only by the Git adapter, so IPC cannot supply author metadata.
    #[serde(skip_deserializing)]
    Comment(Option<Comment>),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub id: String,
    pub kind: String,
    pub master_id: String,
    pub primary_key: Option<PrimaryKey>,
    pub column: Option<String>,
    pub base: Value,
    pub ours: Value,
    pub theirs: Value,
    pub resolution: Option<Resolution>,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeView {
    pub automatically_merged: usize,
    pub conflicts: Vec<Conflict>,
    pub remaining: usize,
    pub error: Option<String>,
}

pub struct MergePlan {
    pub data: ProjectData,
    pub view: MergeView,
}

/// The outer Option distinguishes a conflict from a successfully merged ABSENT.
pub fn three_way<T: Eq + Clone>(base: &T, ours: &T, theirs: &T) -> Option<T> {
    if ours == theirs || theirs == base {
        Some(ours.clone())
    } else if ours == base {
        Some(theirs.clone())
    } else {
        None
    }
}

#[derive(Clone)]
struct Location {
    kind: &'static str,
    master: String,
    key: Option<PrimaryKey>,
    column: Option<String>,
}
impl Location {
    fn new(kind: &'static str, master: &str) -> Self {
        Self {
            kind,
            master: master.into(),
            key: None,
            column: None,
        }
    }
    fn id(&self) -> String {
        serde_json::to_string(&(self.kind, &self.master, &self.key, &self.column)).unwrap()
    }
}
struct Engine<'a> {
    choices: &'a BTreeMap<String, Resolution>,
    view: MergeView,
}
impl Engine<'_> {
    fn conflict<T: Clone + Serialize>(&mut self, loc: Location, b: &T, a: &T, c: &T) -> T {
        let id = loc.id();
        let resolution = self.choices.get(&id).cloned();
        if resolution.is_none() {
            self.view.remaining += 1;
        }
        self.view.conflicts.push(Conflict {
            id,
            kind: loc.kind.into(),
            master_id: loc.master,
            primary_key: loc.key,
            column: loc.column,
            base: serde_json::to_value(b).unwrap(),
            ours: serde_json::to_value(a).unwrap(),
            theirs: serde_json::to_value(c).unwrap(),
            resolution: resolution.clone(),
        });
        match resolution {
            Some(Resolution::Theirs) => c.clone(),
            _ => a.clone(),
        }
    }
    fn value<T: Clone + Eq + Serialize>(&mut self, loc: Location, b: &T, a: &T, c: &T) -> T {
        if let Some(value) = three_way(b, a, c) {
            if a != b || c != b {
                self.view.automatically_merged += 1;
            }
            value
        } else {
            self.conflict(loc, b, a, c)
        }
    }
}

type Row = BTreeMap<String, String>;
type Rows = BTreeMap<PrimaryKey, Row>;
fn rows(table: Option<&Table>, def: &MasterDefinition) -> Result<Rows> {
    let Some(table) = table else {
        return Ok(BTreeMap::new());
    };
    let indices = table.key_indices(def)?;
    Ok(table
        .rows
        .iter()
        .map(|row| {
            (
                Table::key(row, &indices),
                table
                    .columns
                    .iter()
                    .cloned()
                    .zip(row.iter().cloned())
                    .collect(),
            )
        })
        .collect())
}
fn all_keys<T: Ord + Clone, V>(
    b: &BTreeMap<T, V>,
    a: &BTreeMap<T, V>,
    c: &BTreeMap<T, V>,
) -> BTreeSet<T> {
    b.keys().chain(a.keys()).chain(c.keys()).cloned().collect()
}
fn column_values(rows: &Rows, name: &str) -> Vec<(PrimaryKey, String)> {
    rows.iter()
        .filter_map(|(key, row)| row.get(name).map(|v| (key.clone(), v.clone())))
        .collect()
}

fn merge_table(
    e: &mut Engine<'_>,
    id: &str,
    def: &MasterDefinition,
    b: Option<&Table>,
    a: &Table,
    c: &Table,
) -> Result<Table> {
    let br = rows(b, def)?;
    let ar = rows(Some(a), def)?;
    let cr = rows(Some(c), def)?;
    let mut columns = vec![];
    let mut seen = BTreeSet::new();
    for column in b
        .into_iter()
        .flat_map(|t| &t.columns)
        .chain(&a.columns)
        .chain(&c.columns)
    {
        if !seen.insert(column) {
            continue;
        }
        let bp = b.is_some_and(|t| t.columns.contains(column));
        let ap = a.columns.contains(column);
        let cp = c.columns.contains(column);
        let mut loc = Location::new("column", id);
        loc.column = Some(column.clone());
        let keep = if bp && ap != cp {
            let bv = Some(column_values(&br, column));
            let av = ap.then(|| column_values(&ar, column));
            let cv = cp.then(|| column_values(&cr, column));
            let retained = if ap { &ar } else { &cr };
            let modified = retained
                .iter()
                .any(|(key, row)| br.get(key).and_then(|r| r.get(column)) != row.get(column));
            if modified {
                e.conflict(loc, &bv, &av, &cv).is_some()
            } else {
                e.view.automatically_merged += 1;
                false
            }
        } else {
            e.value(loc, &bp, &ap, &cp)
        };
        if keep {
            columns.push(column.clone());
        }
    }
    let mut result = Table {
        columns,
        rows: vec![],
    };
    for key in all_keys(&br, &ar, &cr) {
        let bv = br.get(&key);
        let av = ar.get(&key);
        let cv = cr.get(&key);
        let mut loc = Location::new("row", id);
        loc.key = Some(key.clone());
        if av.is_none() || cv.is_none() {
            // Row deletion compares the row content, not its physical CSV position.
            let row = if let (Some(base), Some(retained)) = (bv, av.or(cv)) {
                let modified = retained.iter().any(|(column, value)| {
                    result.columns.contains(column)
                        && match base.get(column) {
                            Some(before) => before != value,
                            None => !value.is_empty(),
                        }
                });
                if modified {
                    e.conflict(loc, &bv.cloned(), &av.cloned(), &cv.cloned())
                } else {
                    e.view.automatically_merged += 1;
                    None
                }
            } else {
                e.value(loc, &bv.cloned(), &av.cloned(), &cv.cloned())
            };
            if let Some(row) = row {
                result.rows.push(
                    result
                        .columns
                        .iter()
                        .map(|col| row.get(col).cloned().unwrap_or_default())
                        .collect(),
                );
            }
            continue;
        }
        let mut row = vec![];
        for column in &result.columns {
            let base = bv.and_then(|r| r.get(column)).cloned();
            // A retained column that the other side deleted takes the retaining
            // side's values. Column deletion has already been decided above.
            let ours = if !a.columns.contains(column) {
                base.clone()
            } else {
                av.and_then(|r| r.get(column)).cloned()
            };
            let theirs = if !c.columns.contains(column) {
                base.clone()
            } else {
                cv.and_then(|r| r.get(column)).cloned()
            };
            let mut loc = Location::new("cell", id);
            loc.key = Some(key.clone());
            loc.column = Some(column.clone());
            let choice = e.choices.get(&loc.id()).cloned();
            let value = e.value(loc, &base, &ours, &theirs);
            row.push(match choice {
                Some(Resolution::Custom(value)) => crate::lf(&value),
                _ => value.unwrap_or_default(),
            });
        }
        result.rows.push(row);
    }
    result.canonicalize(def)?;
    Ok(result)
}

fn data_master<'a>(data: &'a ProjectData, id: &str) -> Result<Option<&'a Master>> {
    match data.masters.get(id) {
        Some(entry) => entry.data.as_ref().map(Some).ok_or_else(|| {
            format!(
                "{id}: {}",
                entry
                    .error
                    .as_deref()
                    .unwrap_or("Master を読み込めません。")
            )
        }),
        None => Ok(None),
    }
}

pub fn merge_project(
    base: &ProjectData,
    ours: &ProjectData,
    theirs: &ProjectData,
    choices: &BTreeMap<String, Resolution>,
) -> Result<MergePlan> {
    let mut e = Engine {
        choices,
        view: MergeView::default(),
    };
    let git = e.value(
        Location::new("projectConfig", "(Project Settings)"),
        &base.config.git,
        &ours.config.git,
        &theirs.config.git,
    );
    let mut config = ProjectConfig {
        version: 1,
        git,
        masters: BTreeMap::new(),
    };
    let mut masters = BTreeMap::new();
    for id in all_keys(
        &base.config.masters,
        &ours.config.masters,
        &theirs.config.masters,
    ) {
        let bd = base.config.masters.get(&id);
        let ad = ours.config.masters.get(&id);
        let cd = theirs.config.masters.get(&id);
        let bm = data_master(base, &id)?;
        let am = data_master(ours, &id)?;
        let cm = data_master(theirs, &id)?;
        let definition_conflict = match bd {
            Some(b) => ad.is_some_and(|a| a != b) || cd.is_some_and(|c| c != b),
            None => ad.is_some() && cd.is_some() && ad != cd,
        };
        let chosen = if definition_conflict {
            // Definition choice selects the entire Master. Never map rows across PK definitions.
            let b = bd.zip(bm);
            let a = ad.zip(am);
            let c = cd.zip(cm);
            e.conflict(Location::new("projectConfig", &id), &b, &a, &c)
                .map(|(d, m)| (d.clone(), m.clone()))
        } else if let (Some(a), Some(c)) = (am, cm) {
            let def = ad.or(cd).unwrap();
            let table = merge_table(&mut e, &id, def, bm.map(|m| &m.table), &a.table, &c.table)?;
            let mut bc = bm.map(|m| m.comments.clone()).unwrap_or_default();
            let mut ac = a.comments.clone();
            let mut cc = c.comments.clone();
            // Resolve structure first. Deleted identities have no comment decisions.
            for comments in [&mut bc, &mut ac, &mut cc] {
                prune_comments(comments, &table, def)?;
            }
            let mut comments = merge_comments(&mut e, &id, &bc, &ac, &cc);
            comments.validate(&table, def)?;
            Some((def.clone(), Master { table, comments }))
        } else {
            let master = if bm
                .zip(am.or(cm))
                .is_some_and(|(b, retained)| same_master_content(b, retained))
            {
                // Metadata-only updates do not turn Master delete/unchanged into a conflict.
                e.view.automatically_merged += 1;
                None
            } else {
                e.value(
                    Location::new("master", &id),
                    &bm.cloned(),
                    &am.cloned(),
                    &cm.cloned(),
                )
            };
            master.map(|m| (ad.or(cd).or(bd).unwrap().clone(), m))
        };
        if let Some((def, master)) = chosen {
            config.masters.insert(id.clone(), def);
            masters.insert(
                id,
                MasterEntry {
                    data: Some(master),
                    error: None,
                },
            );
        }
    }
    // Cross-Master path collisions can remain until definition choices are made.
    if config.validate().is_err() {
        // Separately valid projects can introduce the same path under different
        // Master IDs. Their combined config is unsafe; choose a coherent project.
        e.view = MergeView::default();
        let data = e.conflict(
            Location::new("projectConfig", "(CSV path / Master ID)"),
            base,
            ours,
            theirs,
        );
        return Ok(MergePlan { data, view: e.view });
    }
    Ok(MergePlan {
        data: ProjectData { config, masters },
        view: e.view,
    })
}

fn prune_comments(comments: &mut Comments, table: &Table, def: &MasterDefinition) -> Result<()> {
    let indices = table.key_indices(def)?;
    let keys: BTreeSet<_> = table.rows.iter().map(|r| Table::key(r, &indices)).collect();
    comments.rows.retain(|r| keys.contains(&r.primary_key));
    comments
        .cells
        .retain(|c| keys.contains(&c.primary_key) && table.columns.contains(&c.column));
    comments.validate(table, def)
}

/// Body alone is semantic content. Metadata follows the selected body; equal
/// bodies use the latest UTC timestamp, with ours winning ties.
pub fn merge_comment(
    base: &Option<Comment>,
    ours: &Option<Comment>,
    theirs: &Option<Comment>,
) -> Option<Option<Comment>> {
    let body = |c: &Option<Comment>| c.as_ref().map(|c| crate::lf(&c.body));
    let (b, a, c) = (body(base), body(ours), body(theirs));
    three_way(&b, &a, &c).map(|selected| {
        selected.as_ref()?;
        if a == c {
            match (ours, theirs) {
                (Some(a), Some(c)) => {
                    let date = |s: &str| chrono::DateTime::parse_from_rfc3339(s).ok();
                    Some(
                        if date(&c.updated_at) > date(&a.updated_at) {
                            c
                        } else {
                            a
                        }
                        .clone(),
                    )
                }
                _ => None,
            }
        } else if selected == a {
            ours.clone()
        } else {
            theirs.clone()
        }
    })
}

fn merge_comments(
    e: &mut Engine<'_>,
    id: &str,
    b: &Comments,
    a: &Comments,
    c: &Comments,
) -> Comments {
    type Key = (Option<PrimaryKey>, Option<String>);
    let entries = |comments: &Comments| -> BTreeMap<Key, Comment> {
        let mut map = BTreeMap::new();
        if let Some(comment) = &comments.table {
            map.insert((None, None), comment.clone());
        }
        for r in &comments.rows {
            map.insert((Some(r.primary_key.clone()), None), r.comment.clone());
        }
        for c in &comments.cells {
            map.insert(
                (Some(c.primary_key.clone()), Some(c.column.clone())),
                c.comment.clone(),
            );
        }
        map
    };
    let (b, a, c) = (entries(b), entries(a), entries(c));
    let mut result = Comments::default();
    for (key, column) in all_keys(&b, &a, &c) {
        let identity = (key.clone(), column.clone());
        let (bv, av, cv) = (
            b.get(&identity).cloned(),
            a.get(&identity).cloned(),
            c.get(&identity).cloned(),
        );
        let loc = Location {
            kind: "comment",
            master: id.into(),
            key: key.clone(),
            column: column.clone(),
        };
        let merged = match merge_comment(&bv, &av, &cv) {
            Some(value) => {
                if bv != av || bv != cv {
                    e.view.automatically_merged += 1;
                }
                value
            }
            None => {
                let choice = e.choices.get(&loc.id()).cloned();
                let value = e.conflict(loc, &bv, &av, &cv);
                match choice {
                    Some(Resolution::Comment(comment)) => comment,
                    _ => value,
                }
            }
        };
        if let Some(comment) = merged {
            match (key, column) {
                (None, _) => result.table = Some(comment),
                (Some(primary_key), None) => result.rows.push(RowComment {
                    primary_key,
                    comment,
                }),
                (Some(primary_key), Some(column)) => result.cells.push(CellComment {
                    primary_key,
                    column,
                    comment,
                }),
            }
        }
    }
    result
}

fn same_master_content(a: &Master, b: &Master) -> bool {
    let content = |m: &Master| {
        let mut entries = BTreeMap::new();
        if let Some(c) = &m.comments.table {
            entries.insert((None, None), crate::lf(&c.body));
        }
        for r in &m.comments.rows {
            entries.insert(
                (Some(r.primary_key.clone()), None),
                crate::lf(&r.comment.body),
            );
        }
        for c in &m.comments.cells {
            entries.insert(
                (Some(c.primary_key.clone()), Some(c.column.clone())),
                crate::lf(&c.comment.body),
            );
        }
        entries
    };
    a.table == b.table && content(a) == content(b)
}
