#[cfg(unix)]
use gamemasterstudio_core::storage;
use gamemasterstudio_core::{
    comments::{CommentTarget, Comments, Identity},
    config::{MasterDefinition, ProjectConfig},
    csv_data::Table,
    project::{git, CellEdit, Operation, Project},
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
        "gamemasterstudio/data.csv",
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
fn edits_autosave_persist_across_all_operation_types() {
    let (dir, mut project) = fixture();
    add(&mut project, "a", "1");
    let original = bytes(dir.path(), "masters/waves.csv");
    apply(&mut project, serde_json::from_value(serde_json::json!({"type":"editCells","masterId":"waves","edits":[{"primaryKey":["a","1"],"column":"name","value":"Slime"},{"primaryKey":["a","1"],"column":"note","value":"001"}]})).unwrap());
    let modified = bytes(dir.path(), "masters/waves.csv");
    assert_ne!(original, modified);
    apply(
        &mut project,
        Operation::AddColumn {
            master_id: "waves".into(),
            name: "hp".into(),
        },
    );
    assert_eq!(
        project.full_snapshot().data.masters["waves"]
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
    let reopened = Project::open(dir.path()).unwrap().full_snapshot();
    assert_eq!(reopened.data, project.full_snapshot().data);
}

#[test]
fn invalid_pk_edits_reject_whole_batch_without_state_or_disk_changes() {
    let (dir, mut project) = fixture();
    add(&mut project, "a", "1");
    add(&mut project, "a", "2");
    comment(
        &mut project,
        CommentTarget::Row {
            primary_key: vec!["a".into(), "1".into()],
        },
        "keep metadata",
    );
    let before = project.full_snapshot();
    let disk = bytes(dir.path(), "masters/waves.csv");
    let metadata = bytes(dir.path(), "gamemasterstudio/comments/waves.json");
    for value in ["2", ""] {
        let operation = serde_json::from_value(serde_json::json!({"type":"editCells","masterId":"waves","edits":[{"primaryKey":["a","1"],"column":"name","value":"bad"},{"primaryKey":["a","1"],"column":"wave","value":value}]})).unwrap();
        let error = project.apply(operation, before.revision).err().unwrap();
        assert!(error.contains(if value.is_empty() {
            "空文字"
        } else {
            "重複"
        }));
        assert_eq!(project.full_snapshot().data, before.data);
        assert_eq!(project.snapshot().revision, before.revision);
        assert_eq!(bytes(dir.path(), "masters/waves.csv"), disk);
        assert_eq!(
            bytes(dir.path(), "gamemasterstudio/comments/waves.json"),
            metadata
        );
    }
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
fn primary_key_fill_renumbers_rows_and_comments_in_one_operation() {
    let (dir, mut project) = fixture();
    for wave in ["1", "2", "3"] {
        add(&mut project, "a", wave);
        let key = vec!["a".into(), wave.into()];
        comment(
            &mut project,
            CommentTarget::Row {
                primary_key: key.clone(),
            },
            wave,
        );
        comment(
            &mut project,
            CommentTarget::Cell {
                primary_key: key,
                column: "wave".into(),
            },
            wave,
        );
    }
    let before = project.full_snapshot();
    let mut edits = vec![];
    for wave in 1..=3 {
        let primary_key = vec!["a".into(), wave.to_string()];
        edits.push(CellEdit {
            primary_key: primary_key.clone(),
            column: "wave".into(),
            value: (wave + 1).to_string(),
        });
        // Still addresses the original row even after its key was edited.
        edits.push(CellEdit {
            primary_key,
            column: "name".into(),
            value: format!("row {wave}"),
        });
    }
    apply(
        &mut project,
        Operation::EditCells {
            master_id: "waves".into(),
            edits,
        },
    );
    let after = project.full_snapshot();
    assert_eq!(after.revision, before.revision + 1);
    let original = before.data.masters["waves"].data.as_ref().unwrap();
    let master = after.data.masters["waves"].data.as_ref().unwrap();
    for (i, row) in master.table.rows.iter().enumerate() {
        let key = vec!["a".to_string(), (i + 2).to_string()];
        assert_eq!(&row[..2], key);
        assert_eq!(row[2], format!("row {}", i + 1));
        assert_eq!(master.comments.rows[i].primary_key, key);
        assert_eq!(
            master.comments.rows[i].comment,
            original.comments.rows[i].comment
        );
        assert_eq!(master.comments.cells[i].primary_key, key);
        assert_eq!(
            master.comments.cells[i].comment,
            original.comments.cells[i].comment
        );
    }
    assert_eq!(
        Project::open(dir.path()).unwrap().full_snapshot().data,
        after.data
    );
}

#[test]
fn composite_key_swaps_move_comments_once_and_preserve_raw_strings() {
    let (dir, mut project) = fixture();
    add(&mut project, "a,b", "001");
    add(&mut project, "a", "b,001");
    let first = vec!["a,b".into(), "001".into()];
    let second = vec!["a".into(), "b,001".into()];
    comment(
        &mut project,
        CommentTarget::Row {
            primary_key: first.clone(),
        },
        "first",
    );
    comment(
        &mut project,
        CommentTarget::Row {
            primary_key: second.clone(),
        },
        "second",
    );
    let mut edits = vec![];
    for (from, to) in [(&first, &second), (&second, &first)] {
        for (column, value) in ["stage", "wave"].into_iter().zip(to) {
            edits.push(CellEdit {
                primary_key: from.clone(),
                column: column.into(),
                value: value.clone(),
            });
        }
    }
    apply(
        &mut project,
        Operation::EditCells {
            master_id: "waves".into(),
            edits,
        },
    );
    let snapshot = project.full_snapshot();
    let master = snapshot.data.masters["waves"].data.as_ref().unwrap();
    for (key, body) in [(&second, "first"), (&first, "second")] {
        assert_eq!(
            master
                .comments
                .rows
                .iter()
                .find(|c| &c.primary_key == key)
                .unwrap()
                .comment
                .body,
            body
        );
    }
    // Comment identities follow the canonical LF form of an edited key.
    apply(
        &mut project,
        Operation::EditCells {
            master_id: "waves".into(),
            edits: vec![CellEdit {
                primary_key: first,
                column: "stage".into(),
                value: "new\r\nstage".into(),
            }],
        },
    );
    let reopened = Project::open(dir.path()).unwrap().full_snapshot();
    let master = reopened.data.masters["waves"].data.as_ref().unwrap();
    assert!(master
        .comments
        .rows
        .iter()
        .any(|c| c.primary_key == vec!["new\nstage", "001"] && c.comment.body == "second"));
}

#[test]
fn row_and_column_delete_remove_comments() {
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
    let original = bytes(dir.path(), "gamemasterstudio/comments/waves.json");
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
    assert!(project.full_snapshot().data.masters["waves"]
        .data
        .as_ref()
        .unwrap()
        .comments
        .cells
        .is_empty());
    apply(
        &mut project,
        Operation::DeleteRows {
            master_id: "waves".into(),
            primary_keys: vec![key],
        },
    );
    let data = project.full_snapshot().data.masters["waves"]
        .data
        .clone()
        .unwrap();
    assert!(
        data.table.rows.is_empty()
            && data.comments.rows.is_empty()
            && data.comments.cells.is_empty()
    );
    comment(&mut project, CommentTarget::Table, "  \r\n ");
    assert!(!dir
        .path()
        .join("gamemasterstudio/comments/waves.json")
        .exists());
}

#[test]
fn comment_edit_preserves_creator_and_updates_repository_local_author() {
    let (dir, mut project) = fixture();
    comment(&mut project, CommentTarget::Table, "first");
    let first = project.full_snapshot().data.masters["waves"]
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
    let second = project.full_snapshot().data.masters["waves"]
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
    let data = project.full_snapshot().data.masters["waves"]
        .data
        .clone()
        .unwrap();
    assert_eq!(data.table.rows[1][2], "Slime");
    assert_eq!(data.comments.rows.len(), 1);
}

#[test]
fn blank_identity_allows_read_but_blocks_edits() {
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
    assert!(reopened.full_snapshot().data.masters["waves"]
        .error
        .is_some());
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
fn empty_master_can_be_configured_but_populated_master_cannot() {
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
    apply(
        &mut project,
        Operation::AddRow {
            master_id: "waves".into(),
            primary_key: vec!["1".into()],
            duplicate_from: None,
        },
    );
    assert!(project
        .apply(
            Operation::ConfigureMaster {
                master_id: "waves".into(),
                path: "other/waves.csv".into(),
                primary_key: vec!["wave".into()]
            },
            project.snapshot().revision
        )
        .is_err());
}

#[test]
fn failed_save_keeps_snapshot_and_revision_unchanged() {
    let (dir, mut project) = fixture();
    add(&mut project, "a", "1");
    let before = project.full_snapshot();
    // A non-directory ancestor fails preparation, before any CSV can be replaced.
    fs::write(dir.path().join("gamemasterstudio/comments"), b"blocker").unwrap();
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
    assert_eq!(bytes(dir.path(), "masters/waves.csv"), original);
    fs::remove_file(dir.path().join("gamemasterstudio/comments")).unwrap();
    assert_eq!(project.full_snapshot().data, before.data);
    assert_eq!(project.snapshot().revision, before.revision);
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
    std::os::unix::fs::symlink(outside.path(), dir.path().join("gamemasterstudio")).unwrap();
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

#[test]
fn create_rows_saves_draft_values_atomically() {
    let (dir, mut project) = fixture();
    let before = project.full_snapshot();
    let disk = bytes(dir.path(), "masters/waves.csv");
    for rows in [
        vec![vec!["", "1", "Slime", ""]],
        vec![vec!["a", "1", "Slime", ""], vec!["a", "1", "Copy", ""]],
        vec![vec!["a", "1"]],
    ] {
        let operation = Operation::CreateRows {
            master_id: "waves".into(),
            rows: rows
                .into_iter()
                .map(|r| r.into_iter().map(String::from).collect())
                .collect(),
        };
        assert!(project.apply(operation, before.revision).is_err());
        assert_eq!(project.full_snapshot().data, before.data);
        assert_eq!(bytes(dir.path(), "masters/waves.csv"), disk);
    }
    apply(
        &mut project,
        serde_json::from_value(serde_json::json!({
            "type": "createRows", "masterId": "waves",
            "rows": [["a", "1", "Slime", "001"], ["a", "2", "Slime", "001"]]
        }))
        .unwrap(),
    );
    assert_eq!(
        bytes(dir.path(), "masters/waves.csv"),
        b"stage,wave,name,note\na,1,Slime,001\na,2,Slime,001\n"
    );
}
