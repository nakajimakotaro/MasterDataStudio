use crate::{
    config::MasterDefinition,
    csv_data::{PrimaryKey, Table},
    project::{CellEdit, Operation, ProjectData, SemanticChange},
    Result,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnScript {
    pub column: String,
    pub script: String,
    pub overrides: Vec<PrimaryKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scripts {
    pub version: u32,
    pub columns: Vec<ColumnScript>,
}
impl Default for Scripts {
    fn default() -> Self {
        Self {
            version: 1,
            columns: vec![],
        }
    }
}
impl Scripts {
    pub fn validate(&mut self, table: &Table, def: &MasterDefinition) -> Result<()> {
        if self.version != 1 {
            return Err("Unsupported Script metadata version".into());
        }
        let indices = table.key_indices(def)?;
        let keys: BTreeSet<_> = table.rows.iter().map(|r| Table::key(r, &indices)).collect();
        let mut columns = BTreeSet::new();
        for entry in &mut self.columns {
            if !table.columns.contains(&entry.column) || def.primary_key.contains(&entry.column) {
                return Err(format!(
                    "Script Column must exist and cannot be a Primary Key: {}",
                    entry.column
                ));
            }
            if !columns.insert(entry.column.clone()) {
                return Err(format!("Duplicate Script Column: {}", entry.column));
            }
            let mut overrides = BTreeSet::new();
            for key in &entry.overrides {
                if key.len() != def.primary_key.len() || !keys.contains(key) {
                    return Err(format!("Invalid Script Override: {} {key:?}", entry.column));
                }
                if !overrides.insert(key.clone()) {
                    return Err(format!(
                        "Duplicate Script Override: {} {key:?}",
                        entry.column
                    ));
                }
            }
            entry.overrides.sort();
            entry.script = crate::lf(&entry.script);
        }
        self.columns
            .sort_by_key(|s| table.columns.iter().position(|c| c == &s.column));
        Ok(())
    }
    pub fn serialize(&self) -> Result<Option<Vec<u8>>> {
        if self.columns.is_empty() && self.version == 1 {
            return Ok(None);
        }
        let mut bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        Ok(Some(bytes))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptTarget {
    pub master_id: String,
    pub primary_key: PrimaryKey,
    pub column: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalculatedCell {
    pub master_id: String,
    #[serde(flatten)]
    pub edit: CellEdit,
}
#[derive(Serialize)]
pub struct PreparedEdit {
    pub data: ProjectData,
    pub targets: Vec<ScriptTarget>,
}

// The backend chooses the complete set of cells that must be evaluated. The
// frontend supplies only results, and never chooses which cells to persist.
pub fn targets(
    operation: &Operation,
    next: &ProjectData,
    safe_mode: bool,
) -> Result<Vec<ScriptTarget>> {
    if safe_mode {
        return Ok(vec![]);
    }
    let (id, keys, column): (&str, Option<BTreeSet<PrimaryKey>>, Option<&str>) = match operation {
        Operation::RecalculateScripts { master_id } => (master_id, None, None),
        Operation::RevertChange { change } => match change {
            SemanticChange::Cell {
                master_id,
                primary_key,
                column,
                ..
            } => {
                let master = next.masters[master_id].data.as_ref().unwrap();
                if master.scripts.columns.iter().any(|s| s.column == *column) {
                    return Ok(vec![]);
                }
                (master_id, Some(BTreeSet::from([primary_key.clone()])), None)
            }
            SemanticChange::DeletedRow {
                master_id,
                primary_key,
            } => (master_id, Some(BTreeSet::from([primary_key.clone()])), None),
            SemanticChange::DeletedColumn { master_id, .. }
            | SemanticChange::DeletedMaster { master_id }
            | SemanticChange::MasterDefinition { master_id, .. } => (master_id, None, None),
            _ => return Ok(vec![]),
        },
        Operation::SetScript {
            master_id,
            column,
            script: Some(_),
        } => (master_id, None, Some(column)),
        Operation::RemoveOverride {
            master_id,
            primary_key,
            column,
        } => (
            master_id,
            Some(BTreeSet::from([primary_key
                .iter()
                .map(|s| crate::lf(s))
                .collect()])),
            Some(column),
        ),
        Operation::EditCells { master_id, edits } => {
            let master = next.masters[master_id].data.as_ref().unwrap();
            let keys = edits
                .iter()
                .filter(|e| !master.scripts.columns.iter().any(|s| s.column == e.column))
                .map(|e| e.primary_key.clone())
                .collect();
            (master_id, Some(keys), None)
        }
        Operation::AddRow {
            master_id,
            primary_key,
            ..
        } => (
            master_id,
            Some(BTreeSet::from([primary_key
                .iter()
                .map(|s| crate::lf(s))
                .collect()])),
            None,
        ),
        Operation::CreateRows { master_id, rows } => {
            let master = next.masters[master_id].data.as_ref().unwrap();
            let indices = master.table.key_indices(&next.config.masters[master_id])?;
            (
                master_id,
                Some(
                    rows.iter()
                        .map(|r| {
                            Table::key(r, &indices)
                                .iter()
                                .map(|s| crate::lf(s))
                                .collect()
                        })
                        .collect(),
                ),
                None,
            )
        }
        _ => return Ok(vec![]),
    };
    let master = next
        .masters
        .get(id)
        .and_then(|e| e.data.as_ref())
        .ok_or("Master がありません。")?;
    if let Some(error) = &master.script_error {
        return Err(format!(
            "Master: {id}\nScript metadata: {error}\nSafe Mode で修正してください。"
        ));
    }
    let indices = master.table.key_indices(&next.config.masters[id])?;
    let mut targets = vec![];
    for row in &master.table.rows {
        let key = Table::key(row, &indices);
        if keys.as_ref().is_some_and(|keys| !keys.contains(&key)) {
            continue;
        }
        for script in &master.scripts.columns {
            if column.is_some_and(|c| c != script.column) || script.overrides.contains(&key) {
                continue;
            }
            targets.push(ScriptTarget {
                master_id: id.into(),
                primary_key: key.clone(),
                column: script.column.clone(),
            });
        }
    }
    Ok(targets)
}
