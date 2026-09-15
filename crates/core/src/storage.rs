use crate::Result;
use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};
use tempfile::NamedTempFile;

/// Managed paths may never traverse symlinks or escape the repository.
pub fn safe_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let mut path = root.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => path.push(part),
            _ => return Err(format!("不正な相対パス: {relative}")),
        }
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "管理対象 path に symlink は使用できません: {}",
                    path.display()
                ))
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(path)
}

pub fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn prepare(path: &Path, bytes: &[u8]) -> Result<NamedTempFile> {
    let parent = path.parent().ok_or("保存先が不正です。")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    if let Ok(meta) = fs::metadata(path) {
        file.as_file()
            .set_permissions(meta.permissions())
            .map_err(|e| e.to_string())?;
    }
    file.write_all(bytes)
        .and_then(|()| file.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    Ok(file)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    prepare(path, bytes)?
        .persist(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

pub struct FileChange {
    pub path: PathBuf,
    pub bytes: Option<Vec<u8>>,
}

/// Prepare every output and rollback copy before replacing any file. In-memory state
/// is committed only after this succeeds. Each individual replacement is atomic.
pub fn transaction(changes: Vec<FileChange>) -> Result<()> {
    transaction_with(changes, |index, path, temp| {
        let _ = index;
        replace(path, temp)
    })
}

fn replace(path: &Path, temp: Option<NamedTempFile>) -> Result<()> {
    if let Some(temp) = temp {
        temp.persist(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
    } else if path.exists() {
        fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

fn transaction_with(
    mut changes: Vec<FileChange>,
    mut commit: impl FnMut(usize, &Path, Option<NamedTempFile>) -> Result<()>,
) -> Result<()> {
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    let mut prepared = vec![];
    for change in changes {
        let old = read_optional(&change.path)?;
        let backup = old
            .as_ref()
            .map(|bytes| prepare(&change.path, bytes))
            .transpose()?;
        let next = change
            .bytes
            .as_ref()
            .map(|bytes| prepare(&change.path, bytes))
            .transpose()?;
        prepared.push((change.path, next, backup));
    }
    for index in 0..prepared.len() {
        let next = prepared[index].1.take();
        if let Err(error) = commit(index, &prepared[index].0, next) {
            let mut rollback_errors = vec![];
            for (path, _, backup) in prepared.iter_mut().take(index).rev() {
                if let Err(error) = replace(path, backup.take()) {
                    rollback_errors.push(error);
                }
            }
            return Err(if rollback_errors.is_empty() {
                format!("保存に失敗しました。変更は取り消されました: {error}")
            } else {
                format!(
                    "保存失敗: {error}。復元にも失敗しました。Project を開き直してください: {}",
                    rollback_errors.join("; ")
                )
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_second_replace_restores_first_file() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.csv");
        let b = dir.path().join("b.json");
        fs::write(&a, b"original a").unwrap();
        fs::write(&b, b"original b").unwrap();
        let result = transaction_with(
            vec![
                FileChange {
                    path: a.clone(),
                    bytes: Some(b"new a".to_vec()),
                },
                FileChange {
                    path: b.clone(),
                    bytes: None,
                },
            ],
            |i, p, t| {
                if i == 1 {
                    Err("injected write failure".into())
                } else {
                    replace(p, t)
                }
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read(a).unwrap(), b"original a");
        assert_eq!(fs::read(b).unwrap(), b"original b");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
