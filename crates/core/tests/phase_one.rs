use gamemasterstudio_core::{
    comments::{CommentTarget, Comments, Identity},
    config::{MasterDefinition, ProjectConfig, CONFIG_PATH},
    csv_data::Table,
    project::{git, Operation, Project},
    storage,
};
use std::{fs, path::Path};

fn definition() -> MasterDefinition {
    MasterDefinition {
        path: "masters/waves.csv".into(),
        primary_key: vec!["stage".into(), "wave".into()],
    }
}

#[test]
fn canonical_csv_sorts_raw_composite_keys_and_preserves_empty_strings() {
    let def = definition();
    let input = b"stage,wave,name,note\nb,2,B,\na,2,A,\na,10,C,\na,001,D,\na,1,E,\n";
    let table = Table::parse(input, &def).unwrap();
    let canonical = table.serialize(&def).unwrap();
    assert_eq!(
        canonical,
        b"stage,wave,name,note\na,001,D,\na,1,E,\na,10,C,\na,2,A,\nb,2,B,\n"
    );
    assert_eq!(
        Table::parse(&canonical, &def)
            .unwrap()
            .serialize(&def)
            .unwrap(),
        canonical
    );
}

#[test]
fn csv_quotes_unicode_commas_quotes_and_normalizes_cell_line_endings() {
    let def = definition();
    let table = Table {
        columns: vec!["stage".into(), "wave".into(), "text".into()],
        rows: vec![vec!["森".into(), "01".into(), "a,\"b\"\r\nc\rd".into()]],
    };
    let bytes = table.serialize(&def).unwrap();
    assert_eq!(
        String::from_utf8(bytes.clone()).unwrap(),
        "stage,wave,text\n森,01,\"a,\"\"b\"\"\nc\nd\"\n"
    );
    let parsed = Table::parse(&bytes, &def).unwrap();
    assert_eq!(parsed.rows[0][2], "a,\"b\"\nc\nd");
}

#[test]
fn tuple_identity_never_uses_a_delimiter_join() {
    let def = definition();
    let table = Table::parse(b"stage,wave\na|b,c\na,b|c\n", &def).unwrap();
    assert_eq!(table.rows.len(), 2);
}

#[test]
fn invalid_csv_and_keys_are_rejected() {
    let def = definition();
    for bad in [
        b"".as_slice(),
        b"\xef\xbb\xbfstage,wave\na,1\n",
        b"stage,wave\na,\xff\n",
        b"stage,wave\na,1,x\n",
        b"stage,wave\na\n",
        b"stage,name\na,foo\n",
        b"stage,wave\na,\n",
        b"stage,wave\na,1\na,1\n",
        b"stage,wave,wave\na,1,1\n",
        b"stage,wave, name\na,1,b\n",
    ] {
        assert!(
            Table::parse(bad, &def).is_err(),
            "Accepted invalid CSV: {bad:?}"
        );
    }
}

#[test]
fn config_validates_identity_paths_version_and_primary_key() {
    let make = |id: &str, path: &str| {
        format!(
            "version: 1\nmasters:\n  {id}:\n    path: '{path}'\n    primaryKey: [stage, wave]\n"
        )
    };
    assert!(ProjectConfig::parse(b"version: 1\nmasters:\n  enemy: {path: a.csv, primaryKey: [id]}\n  Enemy: {path: b.csv, primaryKey: [id]}\n").is_err());
    for path in [
        "/tmp/data.csv",
        "../data.csv",
        "masters/../data.csv",
        "C:\\data.csv",
        ".git/data.csv",
        ".gamemasterstudio/data.csv",
        "masters//data.csv",
        "masters/./data.csv",
    ] {
        assert!(
            ProjectConfig::parse(make("waves", path).as_bytes()).is_err(),
            "Accepted {path}"
        );
    }
    for id in ["../x", "_waves", "wave.id"] {
        assert!(ProjectConfig::parse(make(id, "masters/waves.csv").as_bytes()).is_err());
    }
    let config =
        ProjectConfig::parse(make("stage_enemy", "masters\\waves.csv").as_bytes()).unwrap();
    assert_eq!(config.masters["stage_enemy"].path, "masters/waves.csv");
    assert_eq!(
        ProjectConfig::parse(&config.serialize().unwrap()).unwrap(),
        config
    );
    assert!(ProjectConfig::parse(b"version: 2\nmasters: {}\n").is_err());
    assert!(
        ProjectConfig::parse(b"version: 1\nmasters:\n  a: {path: a.csv, primaryKey: []}\n")
            .is_err()
    );
    assert!(ProjectConfig::parse(
        b"version: 1\nmasters:\n  a: {path: a.csv, primaryKey: [id,id]}\n"
    )
    .is_err());
    assert!(ProjectConfig::parse(b"version: 1\nmasters:\n  a: {path: a.csv, primaryKey: [id]}\n  b: {path: a.csv, primaryKey: [id]}\n").is_err());
}

fn fixture() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::initialize(dir.path()).unwrap();
    project.switch_branch("work", true).unwrap();
    project
        .set_identity("Tester", "tester@example.com")
        .unwrap();
    apply(
        &mut project,
        Operation::CreateMaster {
            master_id: "waves".into(),
            path: "masters/waves.csv".into(),
            primary_key: vec!["stage".into(), "wave".into()],
            columns: vec!["stage".into(), "wave".into(), "name".into(), "note".into()],
        },
    );
    (dir, project)
}

fn apply(project: &mut Project, operation: Operation) {
    project
        .apply(operation, project.snapshot().revision)
        .unwrap();
}

fn add(project: &mut Project, stage: &str, wave: &str) {
    apply(
        project,
        Operation::AddRow {
            master_id: "waves".into(),
            primary_key: vec![stage.into(), wave.into()],
            duplicate_from: None,
        },
    );
}

fn comment(project: &mut Project, target: CommentTarget, body: &str) {
    apply(
        project,
        Operation::SetComment {
            master_id: "waves".into(),
            target,
            body: body.into(),
        },
    );
}

fn bytes(dir: &Path, path: &str) -> Vec<u8> {
    fs::read(dir.join(path)).unwrap()
}

#[test]
fn edits_autosave_and_undo_redo_persist_across_all_operation_types() {
    let (dir, mut project) = fixture();
    add(&mut project, "a", "1");
    let original = bytes(dir.path(), "masters/waves.csv");
    apply(&mut project, serde_json::from_value(serde_json::json!({"type":"editCells","masterId":"waves","edits":[{"primaryKey":["a","1"],"column":"name","value":"Slime"},{"primaryKey":["a","1"],"column":"note","value":"001"}]})).unwrap());
    let modified = bytes(dir.path(), "masters/waves.csv");
    assert_ne!(original, modified);
    project.undo(project.snapshot().revision).unwrap();
    assert_eq!(bytes(dir.path(), "masters/waves.csv"), original);
    project.redo(project.snapshot().revision).unwrap();
    assert_eq!(bytes(dir.path(), "masters/waves.csv"), modified);
    apply(
        &mut project,
        Operation::AddColumn {
            master_id: "waves".into(),
            name: "hp".into(),
        },
    );
    assert_eq!(
        project.snapshot().data.masters["waves"]
            .data
            .as_ref()
            .unwrap()
            .table
            .rows[0]
            .last()
            .unwrap(),
        ""
    );
    apply(
        &mut project,
        Operation::DeleteColumn {
            master_id: "waves".into(),
            name: "hp".into(),
        },
    );
    assert_eq!(bytes(dir.path(), "masters/waves.csv"), modified);
    let reopened = Project::open(dir.path()).unwrap().snapshot();
    assert_eq!(reopened.data, project.snapshot().data);
    assert!(!reopened.can_undo && !reopened.can_redo);
}

#[test]
fn pk_targets_reject_whole_batch_without_history_or_disk_changes() {
    let (dir, mut project) = fixture();
    add(&mut project, "a", "1");
    let before = project.snapshot();
    let disk = bytes(dir.path(), "masters/waves.csv");
    let operation = serde_json::from_value(serde_json::json!({"type":"editCells","masterId":"waves","edits":[{"primaryKey":["a","1"],"column":"name","value":"bad"},{"primaryKey":["a","1"],"column":"wave","value":"2"}]})).unwrap();
    assert!(project.apply(operation, before.revision).is_err());
    assert_eq!(project.snapshot().data, before.data);
    assert_eq!(project.snapshot().revision, before.revision);
    assert_eq!(bytes(dir.path(), "masters/waves.csv"), disk);
    assert!(project
        .apply(
            Operation::DeleteColumn {
                master_id: "waves".into(),
                name: "stage".into()
            },
            before.revision
        )
        .is_err());
    assert!(project
        .apply(
            Operation::AddRow {
                master_id: "waves".into(),
                primary_key: vec!["a".into(), "1".into()],
                duplicate_from: None
            },
            before.revision
        )
        .is_err());
    assert!(project
        .apply(
            Operation::AddColumn {
                master_id: "waves".into(),
                name: "x".into()
            },
            before.revision - 1
        )
        .is_err());
}

#[test]
fn row_and_column_delete_remove_comments_and_undo_restores_exact_metadata() {
    let (dir, mut project) = fixture();
    add(&mut project, "a", "1");
    let key = vec!["a".into(), "1".into()];
    comment(&mut project, CommentTarget::Table, "table");
    comment(
        &mut project,
        CommentTarget::Row {
            primary_key: key.clone(),
        },
        "row",
    );
    comment(
        &mut project,
        CommentTarget::Cell {
            primary_key: key.clone(),
            column: "name".into(),
        },
        "cell",
    );
    let original = bytes(dir.path(), ".gamemasterstudio/comments/waves.json");
    let comments: Comments = serde_json::from_slice(&original).unwrap();
    assert!(original.ends_with(b"\n"));
    assert!(String::from_utf8_lossy(&original).contains("\n  \"version\""));
    assert!(comments.table.as_ref().unwrap().created_at.ends_with('Z'));
    apply(
        &mut project,
        Operation::DeleteColumn {
            master_id: "waves".into(),
            name: "name".into(),
        },
    );
    assert!(project.snapshot().data.masters["waves"]
        .data
        .as_ref()
        .unwrap()
        .comments
        .cells
        .is_empty());
    project.undo(project.snapshot().revision).unwrap();
    assert_eq!(
        bytes(dir.path(), ".gamemasterstudio/comments/waves.json"),
        original
    );
    apply(
        &mut project,
        Operation::DeleteRows {
            master_id: "waves".into(),
            primary_keys: vec![key],
        },
    );
    let data = project.snapshot().data.masters["waves"]
        .data
        .clone()
        .unwrap();
    assert!(
        data.table.rows.is_empty()
            && data.comments.rows.is_empty()
            && data.comments.cells.is_empty()
    );
    project.undo(project.snapshot().revision).unwrap();
    assert_eq!(
        bytes(dir.path(), ".gamemasterstudio/comments/waves.json"),
        original
    );
    comment(&mut project, CommentTarget::Table, "  \r\n ");
    apply(
        &mut project,
        Operation::DeleteRows {
            master_id: "waves".into(),
            primary_keys: vec![vec!["a".into(), "1".into()]],
        },
    );
    assert!(!dir
        .path()
        .join(".gamemasterstudio/comments/waves.json")
        .exists());
    project.undo(project.snapshot().revision).unwrap();
    assert!(dir
        .path()
        .join(".gamemasterstudio/comments/waves.json")
        .exists());
}

#[test]
fn comment_edit_preserves_creator_and_updates_repository_local_author() {
    let (dir, mut project) = fixture();
    comment(&mut project, CommentTarget::Table, "first");
    let first = project.snapshot().data.masters["waves"]
        .data
        .as_ref()
        .unwrap()
        .comments
        .table
        .clone()
        .unwrap();
    project
        .set_identity("Second", "second@example.com")
        .unwrap();
    comment(&mut project, CommentTarget::Table, "second\r\nline");
    let second = project.snapshot().data.masters["waves"]
        .data
        .as_ref()
        .unwrap()
        .comments
        .table
        .clone()
        .unwrap();
    assert_eq!(second.body, "second\nline");
    assert_eq!(first.created_at, second.created_at);
    assert_eq!(first.created_by, second.created_by);
    assert_eq!(second.updated_by.name, "Second");
    assert_eq!(
        git(dir.path(), &["config", "--local", "user.email"]).unwrap(),
        "second@example.com"
    );
    let rev = project.snapshot().revision;
    comment(&mut project, CommentTarget::Table, "second\nline");
    assert_eq!(rev, project.snapshot().revision);
}

#[test]
fn duplication_copies_values_but_not_comments() {
    let (_dir, mut project) = fixture();
    add(&mut project, "a", "1");
    let key = vec!["a".into(), "1".into()];
    apply(&mut project, serde_json::from_value(serde_json::json!({"type":"editCells","masterId":"waves","edits":[{"primaryKey":["a","1"],"column":"name","value":"Slime"}]})).unwrap());
    comment(
        &mut project,
        CommentTarget::Row {
            primary_key: key.clone(),
        },
        "original only",
    );
    apply(
        &mut project,
        Operation::AddRow {
            master_id: "waves".into(),
            primary_key: vec!["a".into(), "2".into()],
            duplicate_from: Some(key),
        },
    );
    let data = project.snapshot().data.masters["waves"]
        .data
        .clone()
        .unwrap();
    assert_eq!(data.table.rows[1][2], "Slime");
    assert_eq!(data.comments.rows.len(), 1);
}

#[test]
fn blank_identity_allows_read_but_blocks_edits_and_undo() {
    let (dir, _) = fixture();
    git(dir.path(), &["config", "--local", "user.name", ""]).unwrap();
    let mut project = Project::open(dir.path()).unwrap();
    assert!(!project.snapshot().identity.is_complete());
    assert!(project
        .apply(
            Operation::AddColumn {
                master_id: "waves".into(),
                name: "hp".into()
            },
            0
        )
        .is_err());
    assert!(project.undo(0).is_err());
}

#[test]
fn invalid_master_is_isolated_and_never_overwritten() {
    let (dir, mut project) = fixture();
    apply(
        &mut project,
        Operation::CreateMaster {
            master_id: "other".into(),
            path: "other.csv".into(),
            primary_key: vec!["id".into()],
            columns: vec!["id".into()],
        },
    );
    fs::write(dir.path().join("masters/waves.csv"), b"broken").unwrap();
    let mut reopened = Project::open(dir.path()).unwrap();
    assert!(reopened.snapshot().data.masters["waves"].error.is_some());
    apply(
        &mut reopened,
        Operation::AddRow {
            master_id: "other".into(),
            primary_key: vec!["1".into()],
            duplicate_from: None,
        },
    );
    assert_eq!(bytes(dir.path(), "masters/waves.csv"), b"broken");
}

#[test]
fn empty_master_can_be_configured_with_undo_but_populated_master_cannot() {
    let (dir, mut project) = fixture();
    apply(
        &mut project,
        Operation::ConfigureMaster {
            master_id: "waves".into(),
            path: "data/waves.csv".into(),
            primary_key: vec!["wave".into()],
        },
    );
    assert!(dir.path().join("data/waves.csv").exists());
    assert!(!dir.path().join("masters/waves.csv").exists());
    project.undo(project.snapshot().revision).unwrap();
    assert!(!dir.path().join("data/waves.csv").exists());
    assert!(dir.path().join("masters/waves.csv").exists());
    add(&mut project, "a", "1");
    assert!(project
        .apply(
            Operation::ConfigureMaster {
                master_id: "waves".into(),
                path: "data/waves.csv".into(),
                primary_key: vec!["wave".into()]
            },
            project.snapshot().revision
        )
        .is_err());
}

#[test]
fn master_creation_undo_removes_csv_and_config_entry_and_redo_restores() {
    let (dir, mut project) = fixture();
    project.undo(project.snapshot().revision).unwrap();
    assert!(!dir.path().join("masters/waves.csv").exists());
    assert!(ProjectConfig::parse(&bytes(dir.path(), CONFIG_PATH))
        .unwrap()
        .masters
        .is_empty());
    project.redo(project.snapshot().revision).unwrap();
    assert!(dir.path().join("masters/waves.csv").exists());
    assert!(Project::initialize(dir.path()).is_err());
}

#[test]
fn failed_save_keeps_snapshot_and_undo_history_unchanged() {
    let (dir, mut project) = fixture();
    add(&mut project, "a", "1");
    let before = project.snapshot();
    // A non-directory ancestor fails preparation, before any CSV can be replaced.
    fs::write(dir.path().join(".gamemasterstudio/comments"), b"blocker").unwrap();
    let original = bytes(dir.path(), "masters/waves.csv");
    assert!(project
        .apply(
            Operation::SetComment {
                master_id: "waves".into(),
                target: CommentTarget::Table,
                body: "cannot save".into()
            },
            before.revision
        )
        .is_err());
    assert_eq!(project.snapshot().data, before.data);
    assert_eq!(project.snapshot().revision, before.revision);
    assert_eq!(project.snapshot().can_undo, before.can_undo);
    assert_eq!(bytes(dir.path(), "masters/waves.csv"), original);
}

#[cfg(unix)]
#[test]
fn symlink_paths_are_rejected_including_metadata_directory() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("linked")).unwrap();
    assert!(storage::safe_path(dir.path(), "linked/data.csv").is_err());
    assert!(storage::safe_path(dir.path(), "../outside.csv").is_err());
    git(dir.path(), &["init", "-b", "main"]).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join(".gamemasterstudio")).unwrap();
    assert!(Project::initialize(dir.path()).is_err());
    assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
}

#[test]
fn comment_json_order_is_tuple_then_column_and_orphans_are_errors() {
    let def = definition();
    let table = Table::parse(b"stage,wave,name,note\na,1,,\na,2,,\n", &def).unwrap();
    let mut comments = Comments::default();
    let identity = Identity {
        name: "Tester".into(),
        email: "tester@example.com".into(),
    };
    for (key, column) in [
        (vec!["a".into(), "2".into()], "note"),
        (vec!["a".into(), "1".into()], "note"),
        (vec!["a".into(), "1".into()], "name"),
    ] {
        comments.set(
            &CommentTarget::Cell {
                primary_key: key,
                column: column.into(),
            },
            "body",
            &identity,
        );
    }
    comments.validate(&table, &def).unwrap();
    assert_eq!(comments.cells[0].column, "name");
    assert_eq!(comments.cells[2].primary_key[1], "2");
    comments.set(
        &CommentTarget::Cell {
            primary_key: vec!["a".into(), "3".into()],
            column: "name".into(),
        },
        "orphan",
        &identity,
    );
    assert!(comments.validate(&table, &def).is_err());
}
