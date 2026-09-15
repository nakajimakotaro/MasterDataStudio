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
        let mut keys = BTreeSet::new();
        for row in &mut self.rows {
            if row.len() != self.columns.len() {
                return Err("CSV の Column 数が一致しません。".into());
            }
            for cell in row.iter_mut() {
                *cell = lf(cell);
            }
            let key = Self::key(row, &indices);
            if key.iter().any(String::is_empty) {
                return Err("Primary Key に空文字は使用できません。".into());
            }
            if !keys.insert(key.clone()) {
                return Err(format!("Primary Key が重複しています: {key:?}"));
            }
        }
        self.rows.sort_by_cached_key(|row| Self::key(row, &indices));
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
        let mut writer = csv::WriterBuilder::new()
            .delimiter(b',')
            .terminator(csv::Terminator::Any(b'\n'))
            .quote_style(csv::QuoteStyle::Necessary)
            .double_quote(true)
            .from_writer(vec![]);
        writer
            .write_record(&table.columns)
            .map_err(|e| e.to_string())?;
        for row in &table.rows {
            writer.write_record(row).map_err(|e| e.to_string())?;
        }
        writer.into_inner().map_err(|e| e.to_string())
    }

    pub fn row_index(&self, key: &PrimaryKey, def: &MasterDefinition) -> Result<usize> {
        let indices = self.key_indices(def)?;
        self.rows
            .iter()
            .position(|r| Self::key(r, &indices) == *key)
            .ok_or_else(|| format!("Row がありません: {key:?}"))
    }
}
