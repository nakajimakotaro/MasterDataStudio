use gamemasterstudio_core::{
    project::{git, CellEdit, MasterUpdate, Operation, Project, ProjectData, ProjectUpdate},
    scripts::CalculatedCell,
};
use std::fs;

fn fixture(rows: usize) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    let mut p = Project::initialize(dir.path()).unwrap();
    p.switch_branch("work", true).unwrap();
    p.set_identity("Tester", "test@example.com").unwrap();
    git(dir.path(), &["config", "commit.gpgsign", "false"]).unwrap();
    for id in ["a", "b"] {
        let revision = p.snapshot().revision;
        p.apply(
            Operation::CreateMaster {
                master_id: id.into(),
                path: format!("{id}.csv"),
                primary_key: vec!["id".into()],
                columns: vec!["id".into(), "value".into(), "computed".into()],
            },
            revision,
        )
        .unwrap();
    }
    let csv = format!(
        "id,value,computed\n{}",
        (0..rows)
            .map(|i| format!("{i:06},{},\n", "x".repeat(80)))
            .collect::<String>()
    );
    for id in ["a", "b"] {
        fs::write(dir.path().join(format!("{id}.csv")), &csv).unwrap();
    }
    let mut p = Project::open(dir.path()).unwrap();
    p.commit("initial", false).unwrap();
    (dir, p)
}

fn edit() -> Operation {
    Operation::EditCells {
        master_id: "a".into(),
        edits: vec![CellEdit {
            primary_key: vec!["000000".into()],
            column: "value".into(),
            value: "edited".into(),
        }],
    }
}

fn apply_delta(data: &mut ProjectData, update: ProjectUpdate) {
    data.config = update.data.config;
    for (id, change) in update.data.masters {
        match change {
            None => {
                data.masters.remove(&id);
            }
            Some(MasterUpdate::Replace { entry }) => {
                data.masters.insert(id, entry);
            }
            Some(MasterUpdate::Rows {
                rows,
                row_count,
                comments,
                scripts,
                script_error,
                error,
            }) => {
                let entry = data.masters.get_mut(&id).unwrap();
                let master = entry.data.as_mut().unwrap();
                master.table.rows.resize(row_count, vec![]);
                for (index, row) in rows {
                    master.table.rows[index] = row;
                }
                master.comments = comments;
                master.scripts = scripts;
                master.script_error = script_error;
                entry.error = error;
            }
        }
    }
}

#[test]
fn updates_reconstruct_full_snapshots_through_structural_edits() {
    let (_dir, mut p) = fixture(3);
    let mut cached = p.snapshot();
    let operations = vec![
        edit(),
        Operation::CreateRows {
            master_id: "a".into(),
            rows: vec![vec!["000000a".into(), "new".into(), "".into()]],
        },
        Operation::DeleteRows {
            master_id: "a".into(),
            primary_keys: vec![vec!["000000".into()]],
        },
        Operation::AddColumn {
            master_id: "a".into(),
            name: "extra".into(),
        },
        Operation::DeleteRows {
            master_id: "a".into(),
            primary_keys: vec![
                vec!["000000a".into()],
                vec!["000001".into()],
                vec!["000002".into()],
            ],
        },
        Operation::ConfigureMaster {
            master_id: "a".into(),
            path: "renamed.csv".into(),
            primary_key: vec!["id".into()],
        },
        Operation::SetProtectedBranches {
            patterns: vec!["release/*".into()],
        },
        Operation::CreateMaster {
            master_id: "new".into(),
            path: "new.csv".into(),
            primary_key: vec!["id".into()],
            columns: vec!["id".into()],
        },
        Operation::RevertChange {
            change: gamemasterstudio_core::project::SemanticChange::AddedMaster {
                master_id: "new".into(),
            },
        },
    ];
    for operation in operations {
        let update = p
            .apply_calculated_update(operation, vec![], cached.revision)
            .unwrap();
        assert_eq!(update.data.base_revision, cached.revision);
        assert!(!update.data.masters.contains_key("b"));
        cached.revision = update.revision;
        apply_delta(&mut cached.data, update);
        assert_eq!(cached.data, p.snapshot().data);
    }
}

#[test]
fn script_preview_sends_only_target_rows_and_updates_are_atomic() {
    let (_dir, mut p) = fixture(3);
    let revision = p.snapshot().revision;
    let op = Operation::SetScript {
        master_id: "a".into(),
        column: "computed".into(),
        script: Some("return row.value;".into()),
    };
    let preview = p.preview_scripts(op.clone(), revision).unwrap();
    assert_eq!(preview.targets.len(), 3);
    let calculated = preview
        .targets
        .iter()
        .map(|t| CalculatedCell {
            master_id: t.master_id.clone(),
            edit: CellEdit {
                primary_key: t.primary_key.clone(),
                column: t.column.clone(),
                value: "result".into(),
            },
        })
        .collect();
    p.apply_calculated_update(op, calculated, revision).unwrap();
    let before = p.snapshot();
    let preview = p.preview_scripts(edit(), before.revision).unwrap();
    assert_eq!(preview.data.masters.len(), 1);
    assert_eq!(preview.data.config.masters.len(), 1);
    let rows = &preview.data.masters["a"].data.as_ref().unwrap().table.rows;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][1], "edited");
    assert_eq!(preview.targets.len(), 1);
    assert!(p
        .apply_calculated_update(edit(), vec![], before.revision)
        .is_err());
    assert_eq!(p.snapshot().data, before.data);
    let update = p
        .apply_calculated_update(
            edit(),
            vec![CalculatedCell {
                master_id: "a".into(),
                edit: CellEdit {
                    primary_key: vec!["000000".into()],
                    column: "computed".into(),
                    value: "edited".into(),
                },
            }],
            before.revision,
        )
        .unwrap();
    let mut cached = before.data;
    apply_delta(&mut cached, update);
    assert_eq!(cached, p.snapshot().data);
    assert!(p
        .apply_calculated_update(edit(), vec![], before.revision)
        .is_err());
    p.safe_mode = true;
    let safe = p.preview_scripts(edit(), p.snapshot().revision).unwrap();
    assert!(safe.targets.is_empty());
    assert!(safe.data.masters.is_empty());
}

#[test]
fn one_cell_payload_does_not_include_twenty_thousand_unchanged_rows() {
    let (_dir, mut p) = fixture(10_000);
    let before = p.snapshot();
    let revision = before.revision;
    let full_preview_bytes = serde_json::to_vec(&p.preview(edit(), revision).unwrap())
        .unwrap()
        .len();
    let compact_preview = p.preview_scripts(edit(), revision).unwrap();
    let preview_bytes = serde_json::to_vec(&compact_preview).unwrap().len();
    assert!(compact_preview.data.masters.is_empty());
    let update = p.apply_calculated_update(edit(), vec![], revision).unwrap();
    let update_bytes = serde_json::to_vec(&update).unwrap().len();
    let full_bytes = serde_json::to_vec(&p.snapshot()).unwrap().len();
    assert_eq!(update.data.masters.len(), 1);
    match update.data.masters["a"].as_ref().unwrap() {
        MasterUpdate::Rows {
            rows, row_count, ..
        } => {
            assert_eq!(rows.len(), 1);
            assert_eq!(*row_count, 10_000);
        }
        _ => panic!("cell edit must use a row patch"),
    }
    assert!(update_bytes * 100 < full_bytes);
    assert!(preview_bytes * 100 < full_preview_bytes);
    eprintln!("20,000 rows: preview {full_preview_bytes} -> {preview_bytes} bytes; edit result {full_bytes} -> {update_bytes} bytes");
    let mut cached = before.data;
    apply_delta(&mut cached, update);
    assert_eq!(cached, p.snapshot().data);
    let revision = p.snapshot().revision;
    let no_op = p.apply_calculated_update(edit(), vec![], revision).unwrap();
    assert_eq!(no_op.revision, revision);
    assert!(no_op.data.masters.is_empty());
}

#[test]
fn git_metadata_and_script_changes_are_refreshed_on_demand() {
    let (_dir, mut p) = fixture(3);
    let before = p.snapshot();
    let updated = p
        .apply_calculated_update(edit(), vec![], before.revision)
        .unwrap();
    let wire = serde_json::to_value(&updated).unwrap();
    for field in ["git", "changes", "scriptChanges", "changesError"] {
        assert!(wire.get(field).is_none(), "edit response included {field}");
    }
    p.safe_mode = true;
    let updated = p
        .apply_calculated_update(
            Operation::SetScript {
                master_id: "a".into(),
                column: "computed".into(),
                script: Some("return row.value;".into()),
            },
            vec![],
            updated.revision,
        )
        .unwrap();
    let state = p.repository_state().unwrap();
    assert_eq!(state.revision, updated.revision);
    assert!(state.git.tracked_dirty);
    assert_eq!(state.changes.len(), 1);
    assert_eq!(
        state.script_changes,
        vec!["gamemasterstudio/scripts/a.json"]
    );
    let review = p.change_review(updated.revision).unwrap();
    assert_eq!(
        serde_json::to_value(&review.changes).unwrap(),
        serde_json::to_value(&state.changes).unwrap()
    );
    assert_eq!(review.script_changes, state.script_changes);
}

#[test]
fn lightweight_edits_still_reject_external_protected_branch_and_merge_changes() {
    let (dir, mut p) = fixture(3);
    let revision = p.snapshot().revision;
    let update = p
        .apply_calculated_update(
            Operation::SetProtectedBranches {
                patterns: vec!["locked".into()],
            },
            vec![],
            revision,
        )
        .unwrap();
    git(dir.path(), &["checkout", "-b", "locked"]).unwrap();
    assert!(p.preview_scripts(edit(), update.revision).is_err());
    assert!(p
        .apply_calculated_update(edit(), vec![], update.revision)
        .is_err());
    git(dir.path(), &["checkout", "work"]).unwrap();
    let head = git(dir.path(), &["rev-parse", "HEAD"]).unwrap();
    fs::write(dir.path().join(".git/MERGE_HEAD"), format!("{head}\n")).unwrap();
    assert!(p.preview_scripts(edit(), update.revision).is_err());
    assert!(p
        .apply_calculated_update(edit(), vec![], update.revision)
        .is_err());
}

#[test]
fn ordinary_edits_only_run_git_editability_checks() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("git-trace.log");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "git_trace_edit_scenario",
            "--ignored",
            "--nocapture",
        ])
        .env("GIT_TRACE", &log)
        .env("GMS_EDIT_TRACE_LOG", &log)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let trace = fs::read_to_string(log).unwrap();
    let mut calls = 0;
    for line in trace.lines() {
        if let Some((_, command)) = line.split_once("built-in: git ") {
            calls += 1;
            assert!(
                command == "branch --show-current" || command == "rev-parse -q --verify MERGE_HEAD",
                "unexpected Git work during edit: {command}"
            );
        }
    }
    assert!(calls > 0, "Git trace was empty: {trace}");
}

// A subprocess keeps Git tracing isolated from the parallel test suite.
#[test]
#[ignore]
fn git_trace_edit_scenario() {
    let (_dir, mut p) = fixture(3);
    let revision = p.snapshot().revision;
    fs::write(std::env::var("GMS_EDIT_TRACE_LOG").unwrap(), "").unwrap();
    p.preview_scripts(edit(), revision).unwrap();
    let update = p.apply_calculated_update(edit(), vec![], revision).unwrap();
    p.apply_calculated_update(edit(), vec![], update.revision)
        .unwrap();
}
