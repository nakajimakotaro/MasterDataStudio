use crate::{
    config::MasterDefinition,
    csv_data::{PrimaryKey, Table},
    lf, Result,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub name: String,
    pub email: String,
}

impl Identity {
    pub fn is_complete(&self) -> bool {
        !self.name.trim().is_empty() && !self.email.trim().is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Comment {
    pub body: String,
    pub created_by: Identity,
    pub created_at: String,
    pub updated_by: Identity,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RowComment {
    pub primary_key: PrimaryKey,
    pub comment: Comment,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellComment {
    pub primary_key: PrimaryKey,
    pub column: String,
    pub comment: Comment,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comments {
    pub version: u32,
    pub table: Option<Comment>,
    pub rows: Vec<RowComment>,
    pub cells: Vec<CellComment>,
}

impl Default for Comments {
    fn default() -> Self {
        Self {
            version: 1,
            table: None,
            rows: vec![],
            cells: vec![],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CommentTarget {
    Table,
    Row {
        #[serde(rename = "primaryKey")]
        primary_key: PrimaryKey,
    },
    Cell {
        #[serde(rename = "primaryKey")]
        primary_key: PrimaryKey,
        column: String,
    },
}

impl Comments {
    pub fn validate(&mut self, table: &Table, def: &MasterDefinition) -> Result<()> {
        if self.version != 1 {
            return Err("未対応の Comment version です。".into());
        }
        let mut rows = BTreeSet::new();
        for row in &self.rows {
            table.row_index(&row.primary_key, def)?;
            if !rows.insert(&row.primary_key) {
                return Err("Row Comment identity が重複しています。".into());
            }
        }
        let mut cells = BTreeSet::new();
        for cell in &self.cells {
            table.row_index(&cell.primary_key, def)?;
            if !table.columns.contains(&cell.column) {
                return Err(format!("Comment の Column がありません: {}", cell.column));
            }
            if !cells.insert((&cell.primary_key, &cell.column)) {
                return Err("Cell Comment identity が重複しています。".into());
            }
        }
        for comment in self
            .table
            .iter_mut()
            .chain(self.rows.iter_mut().map(|r| &mut r.comment))
            .chain(self.cells.iter_mut().map(|c| &mut c.comment))
        {
            comment.body = lf(&comment.body);
            if comment.body.trim().is_empty()
                || !comment.created_by.is_complete()
                || !comment.updated_by.is_complete()
            {
                return Err("Comment の本文または作者が不正です。".into());
            }
            for value in [&mut comment.created_at, &mut comment.updated_at] {
                *value = DateTime::parse_from_rfc3339(value)
                    .map_err(|e| format!("Comment の日時: {e}"))?
                    .with_timezone(&Utc)
                    .to_rfc3339_opts(SecondsFormat::Millis, true);
            }
        }
        self.rows.sort_by(|a, b| a.primary_key.cmp(&b.primary_key));
        self.cells
            .sort_by(|a, b| (&a.primary_key, &a.column).cmp(&(&b.primary_key, &b.column)));
        Ok(())
    }

    pub fn serialize(&self) -> Result<Option<Vec<u8>>> {
        if self.table.is_none() && self.rows.is_empty() && self.cells.is_empty() {
            return Ok(None);
        }
        let mut bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        Ok(Some(bytes))
    }

    pub fn set(&mut self, target: &CommentTarget, body: &str, identity: &Identity) {
        let old = match target {
            CommentTarget::Table => self.table.clone(),
            CommentTarget::Row { primary_key } => self
                .rows
                .iter()
                .find(|r| r.primary_key == *primary_key)
                .map(|r| r.comment.clone()),
            CommentTarget::Cell {
                primary_key,
                column,
            } => self
                .cells
                .iter()
                .find(|c| c.primary_key == *primary_key && c.column == *column)
                .map(|c| c.comment.clone()),
        };
        let body = lf(body);
        if old.as_ref().map(|c| c.body.as_str()) == Some(body.as_str()) {
            return;
        }
        let comment = if body.trim().is_empty() {
            None
        } else {
            let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
            Some(Comment {
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
            })
        };
        match target {
            CommentTarget::Table => self.table = comment,
            CommentTarget::Row { primary_key } => {
                self.rows.retain(|r| r.primary_key != *primary_key);
                if let Some(comment) = comment {
                    self.rows.push(RowComment {
                        primary_key: primary_key.clone(),
                        comment,
                    });
                }
            }
            CommentTarget::Cell {
                primary_key,
                column,
            } => {
                self.cells
                    .retain(|c| c.primary_key != *primary_key || c.column != *column);
                if let Some(comment) = comment {
                    self.cells.push(CellComment {
                        primary_key: primary_key.clone(),
                        column: column.clone(),
                        comment,
                    });
                }
            }
        }
    }

    pub fn delete_rows(&mut self, keys: &[PrimaryKey]) {
        self.rows.retain(|r| !keys.contains(&r.primary_key));
        self.cells.retain(|c| !keys.contains(&c.primary_key));
    }
}
