use crate::{
    config::{validate_column, MasterDefinition},
    lf, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub type PrimaryKey = Vec<String>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn key_indices(&self, def: &MasterDefinition) -> Result<Vec<usize>> {
        def.primary_key
            .iter()
            .map(|key| {
                self.columns
                    .iter()
                    .position(|c| c == key)
                    .ok_or_else(|| format!("Primary Key Column がありません: {key}"))
            })
            .collect()
    }

    pub fn key(row: &[String], indices: &[usize]) -> PrimaryKey {
        indices.iter().map(|&i| row[i].clone()).collect()
    }

    pub fn canonicalize(&mut self, def: &MasterDefinition) -> Result<()> {
        if self.columns.is_empty() {
            return Err("CSV header が必要です。".into());
        }
        let mut columns = BTreeSet::new();
        for column in &self.columns {
            validate_column(column)?;
            if !columns.insert(column) {
                return Err(format!("Column が重複しています: {column}"));
            }
        }
        let indices = self.key_indices(def)?;
        if indices.is_empty() {
            return Err("Primary Key が必要です。".into());
        }
        for row in &mut self.rows {
            if row.len() != self.columns.len() {
                return Err("CSV の Column 数が一致しません。".into());
            }
            for cell in row.iter_mut() {
                if cell.contains('\r') {
                    *cell = lf(cell);
                }
            }
            if indices.iter().any(|&i| row[i].is_empty()) {
                return Err("Primary Key に空文字は使用できません。".into());
            }
        }
        let compare = |a: &Vec<String>, b: &Vec<String>| {
            indices
                .iter()
                .map(|&i| &a[i])
                .cmp(indices.iter().map(|&i| &b[i]))
        };
        self.rows.sort_by(compare);
        if let Some(pair) = self
            .rows
            .windows(2)
            .find(|pair| compare(&pair[0], &pair[1]).is_eq())
        {
            return Err(format!(
                "Primary Key が重複しています: {:?}",
                Self::key(&pair[0], &indices)
            ));
        }
        Ok(())
    }

    pub fn parse(bytes: &[u8], def: &MasterDefinition) -> Result<Self> {
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            return Err("CSV の UTF-8 BOM は使用できません。".into());
        }
        std::str::from_utf8(bytes).map_err(|e| format!("CSV は UTF-8 が必要です: {e}"))?;
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(b',')
            .has_headers(true)
            .flexible(false)
            .from_reader(bytes);
        let columns = reader
            .headers()
            .map_err(|e| format!("CSV header: {e}"))?
            .iter()
            .map(str::to_owned)
            .collect();
        let rows = reader
            .records()
            .map(|r| {
                r.map(|r| r.iter().map(str::to_owned).collect())
                    .map_err(|e| format!("CSV: {e}"))
            })
            .collect::<Result<_>>()?;
        let mut table = Self { columns, rows };
        table.canonicalize(def)?;
        Ok(table)
    }

    pub fn serialize(&self, def: &MasterDefinition) -> Result<Vec<u8>> {
        let mut table = self.clone();
        table.canonicalize(def)?;
        table.serialize_canonical()
    }

    /// Internal persistence has already validated and canonicalized the table.
    pub(crate) fn serialize_canonical(&self) -> Result<Vec<u8>> {
        let mut writer = csv::WriterBuilder::new()
            .delimiter(b',')
            .terminator(csv::Terminator::Any(b'\n'))
            .quote_style(csv::QuoteStyle::Necessary)
            .double_quote(true)
            .from_writer(vec![]);
        writer
            .write_record(&self.columns)
            .map_err(|e| e.to_string())?;
        for row in &self.rows {
            writer.write_record(row).map_err(|e| e.to_string())?;
        }
        writer.into_inner().map_err(|e| e.to_string())
    }

    /// Look up a key in a table ordered by parse() or canonicalize().
    pub fn row_index(&self, key: &PrimaryKey, def: &MasterDefinition) -> Result<usize> {
        let indices = self.key_indices(def)?;
        self.rows
            .binary_search_by(|row| indices.iter().map(|&i| &row[i]).cmp(key.iter()))
            .map_err(|_| format!("Row がありません: {key:?}"))
    }
}
