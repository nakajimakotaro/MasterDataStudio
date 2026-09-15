use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CONFIG_PATH: &str = "gamemasterstudio/project.yaml";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MasterDefinition {
    pub path: String,
    pub primary_key: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitConfig {
    pub protected_branches: Vec<String>,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            protected_branches: vec!["main".into()],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub version: u32,
    #[serde(default)]
    pub git: GitConfig,
    pub masters: BTreeMap<String, MasterDefinition>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            version: 1,
            git: GitConfig::default(),
            masters: BTreeMap::new(),
        }
    }
}

pub fn validate_column(name: &str) -> Result<()> {
    if name.is_empty() || name.trim() != name || name.contains(['\r', '\n', '\0']) {
        return Err("Column 名は空白で始終せず、改行を含まない名前にしてください。".into());
    }
    Ok(())
}

pub fn normalize_csv_path(path: &str) -> Result<String> {
    let path = path.replace('\\', "/");
    if path.is_empty()
        || path.starts_with('/')
        || path.contains([':', '\0'])
        || path.split('/').any(|p| {
            p.is_empty()
                || p == ".."
                || p == "."
                || p.eq_ignore_ascii_case(".git")
                || p.eq_ignore_ascii_case("gamemasterstudio")
        })
        || !path.ends_with(".csv")
    {
        return Err("CSV path は Repository 内の相対パス（.csv）にしてください。絶対パス、..、予約ディレクトリは使用できません。".into());
    }
    Ok(path)
}

impl ProjectConfig {
    pub fn validate(&mut self) -> Result<()> {
        if self.version != 1 {
            return Err(format!("未対応の Project Config version: {}", self.version));
        }
        let mut patterns = BTreeSet::new();
        for pattern in &self.git.protected_branches {
            if pattern.is_empty()
                || pattern.trim() != pattern
                || pattern.contains(['\r', '\n', '\0'])
            {
                return Err(
                    "Protected Branch pattern は空白で始終しない名前にしてください。".into(),
                );
            }
            glob::Pattern::new(pattern).map_err(|e| format!("Protected Branch pattern: {e}"))?;
            if !patterns.insert(pattern) {
                return Err(format!(
                    "Protected Branch pattern が重複しています: {pattern}"
                ));
            }
        }
        let mut paths = BTreeSet::new();
        let mut comment_paths = BTreeSet::new();
        for (id, def) in &mut self.masters {
            if id.is_empty()
                || !id.as_bytes()[0].is_ascii_alphanumeric()
                || !id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            {
                return Err(format!("Master ID が不正です: {id}"));
            }
            if !comment_paths.insert(id.to_ascii_lowercase()) {
                return Err(format!(
                    "Comment file の path が衝突する Master ID です: {id}"
                ));
            }
            def.path = normalize_csv_path(&def.path)?;
            // Case-fold as well, so repositories remain portable to case-insensitive filesystems.
            if !paths.insert(def.path.to_lowercase()) {
                return Err(format!("CSV path が重複しています: {}", def.path));
            }
            if def.primary_key.is_empty() {
                return Err(format!("{id}: Primary Key を 1 つ以上指定してください。"));
            }
            let mut keys = BTreeSet::new();
            for key in &def.primary_key {
                validate_column(key)?;
                if !keys.insert(key) {
                    return Err(format!("{id}: Primary Key Column が重複しています。"));
                }
            }
        }
        Ok(())
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let mut config: Self =
            serde_yaml::from_slice(bytes).map_err(|e| format!("Project Config: {e}"))?;
        config.validate()?;
        Ok(config)
    }

    pub fn serialize(&self) -> Result<Vec<u8>> {
        serde_yaml::to_string(self)
            .map(String::into_bytes)
            .map_err(|e| e.to_string())
    }
}
