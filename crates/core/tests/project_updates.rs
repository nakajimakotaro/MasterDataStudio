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
    let mut cached = p.full_snapshot();
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
        assert_eq!(cached.data, p.full_snapshot().data);
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
    let before = p.full_snapshot();
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
    assert_eq!(p.full_snapshot().data, before.data);
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
    assert_eq!(cached, p.full_snapshot().data);
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
    let before = p.full_snapshot();
    let revision = before.revision;
    let full_preview_bytes = serde_json::to_vec(&p.preview(edit(), revision).unwrap())
        .unwrap()
        .len();
    let compact_preview = p.preview_scripts(edit(), revision).unwrap();
    let preview_bytes = serde_json::to_vec(&compact_preview).unwrap().len();
    assert!(compact_preview.data.masters.is_empty());
    let update = p.apply_calculated_update(edit(), vec![], revision).unwrap();
    let update_bytes = serde_json::to_vec(&update).unwrap().len();
    let full_bytes = serde_json::to_vec(&p.full_snapshot()).unwrap().len();
    let workspace_bytes = serde_json::to_vec(&p.snapshot()).unwrap().len();
    assert!(workspace_bytes * 100 < full_bytes);
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
    eprintln!("Workspace: {workspace_bytes} bytes vs full read {full_bytes} bytes");
    eprintln!("20,000 rows: preview {full_preview_bytes} -> {preview_bytes} bytes; edit result {full_bytes} -> {update_bytes} bytes");
    let mut cached = before.data;
    apply_delta(&mut cached, update);
    assert_eq!(cached, p.full_snapshot().data);
    let revision = p.snapshot().revision;
    let no_op = p.apply_calculated_update(edit(), vec![], revision).unwrap();
    assert_eq!(no_op.revision, revision);
    assert!(no_op.data.masters.is_empty());
}

#[test]
fn git_metadata_and_script_changes_are_refreshed_on_demand() {
    let (_dir, mut p) = fixture(3);
    let before = p.full_snapshot();
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
    let wire = serde_json::to_value(&state).unwrap();
    assert!(wire.get("changes").is_none());
    let summary = p.review_summary(updated.revision).unwrap();
    assert_eq!(summary.masters, vec!["a"]);
    assert_eq!(
        summary.script_changes,
        vec!["gamemasterstudio/scripts/a.json"]
    );
    let review = p.review_master(updated.revision, "a").unwrap();
    assert_eq!(review.changes.len(), 1);
    assert_eq!(review.after.masters.len(), 1);
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

#[test]
fn workspace_and_git_reads_do_not_load_or_retain_master_contents() {
    let (dir, p) = fixture(3);
    // Table contents can become unreadable without breaking the workspace or Git dialogs.
    fs::write(dir.path().join("b.csv"), "invalid CSV").unwrap();
    let snapshot = p.snapshot();
    assert_eq!(snapshot.data.config.masters.len(), 2);
    assert!(snapshot.data.masters.is_empty());
    let wire = serde_json::to_value(&snapshot).unwrap();
    for field in ["changes", "changesError", "scriptChanges"] {
        assert!(wire.get(field).is_none());
    }
    assert!(p.repository_state().unwrap().git.tracked_dirty);
    let summary = p.review_summary(snapshot.revision).unwrap();
    assert_eq!(summary.masters, vec!["b"]);
    assert!(p.master("b", snapshot.revision).unwrap().error.is_some());
    assert!(p.master("a", snapshot.revision).unwrap().data.is_some());
    assert!(p.snapshot().data.masters.is_empty());
    assert!(p.master("a", snapshot.revision + 1).is_err());
}

#[test]
fn edits_and_reviews_read_only_the_target_and_use_saved_files() {
    let (dir, mut p) = fixture(3);
    fs::write(dir.path().join("b.csv"), "unrelated invalid CSV").unwrap();
    // Reading and editing uses the file, not a project-open copy of its rows.
    fs::write(
        dir.path().join("a.csv"),
        "id,value,computed\n000000,disk value,\n000001,keep me,\n",
    )
    .unwrap();
    let revision = p.snapshot().revision;
    assert_eq!(
        p.master("a", revision).unwrap().data.unwrap().table.rows[0][1],
        "disk value"
    );
    let update = p.apply_calculated_update(edit(), vec![], revision).unwrap();
    assert_eq!(update.data.masters.len(), 1);
    let master = p.master("a", update.revision).unwrap().data.unwrap();
    assert_eq!(master.table.rows[0][1], "edited");
    assert_eq!(master.table.rows[1][1], "keep me");
    assert_eq!(
        fs::read_to_string(dir.path().join("b.csv")).unwrap(),
        "unrelated invalid CSV"
    );
    let review = p.review_master(update.revision, "a").unwrap();
    assert_eq!(review.before.as_ref().unwrap().masters.len(), 1);
    assert_eq!(review.after.masters.len(), 1);
    assert!(review.changes.iter().any(|c| matches!(c,
        gamemasterstudio_core::project::SemanticChange::Cell { master_id, after, .. }
        if master_id == "a" && after == "edited")));
    assert!(p.review_master(update.revision, "b").is_err());
    assert!(p.review_master(revision, "a").is_err());
    assert!(p.snapshot().data.masters.is_empty());
}

#[test]
fn review_summary_handles_untracked_staged_deleted_and_config_changes() {
    let (dir, mut p) = fixture(3);
    let revision = p.snapshot().revision;
    let update = p
        .apply_calculated_update(
            Operation::CreateMaster {
                master_id: "new".into(),
                path: "new.csv".into(),
                primary_key: vec!["id".into()],
                columns: vec!["id".into()],
            },
            vec![],
            revision,
        )
        .unwrap();
    assert_eq!(
        p.review_summary(update.revision).unwrap().masters,
        vec!["new"]
    );
    git(dir.path(), &["add", "a.csv"]).unwrap();
    p.apply_calculated_update(edit(), vec![], update.revision)
        .unwrap();
    git(dir.path(), &["add", "a.csv"]).unwrap();
    assert_eq!(
        p.review_summary(p.snapshot().revision).unwrap().masters,
        vec!["a", "new"]
    );
    // Delete a master definition and its CSV, then reopen the updated project metadata.
    let mut config = p.snapshot().data.config;
    config.masters.remove("b");
    fs::write(
        dir.path().join("gamemasterstudio/project.yaml"),
        config.serialize().unwrap(),
    )
    .unwrap();
    fs::remove_file(dir.path().join("b.csv")).unwrap();
    let p = Project::open(dir.path()).unwrap();
    assert_eq!(p.review_summary(0).unwrap().masters, vec!["a", "b", "new"]);
    let review = p.review_master(0, "b").unwrap();
    assert!(review.after.masters.is_empty());
    assert_eq!(review.before.unwrap().masters.len(), 1);
    assert!(matches!(
        review.changes[0],
        gamemasterstudio_core::project::SemanticChange::DeletedMaster { .. }
    ));
}

#[test]
fn settings_review_and_script_discovery_do_not_require_csv_data() {
    let (dir, mut p) = fixture(3);
    fs::write(dir.path().join("b.csv"), "invalid").unwrap();
    fs::create_dir_all(dir.path().join("gamemasterstudio/scripts")).unwrap();
    fs::write(
        dir.path().join("gamemasterstudio/scripts/a.json"),
        r#"{"version":1,"columns":[]}"#,
    )
    .unwrap();
    assert_eq!(p.script_masters().unwrap(), vec!["a"]);
    let update = p
        .apply_calculated_update(
            Operation::SetProtectedBranches {
                patterns: vec!["release/*".into()],
            },
            vec![],
            p.snapshot().revision,
        )
        .unwrap();
    assert!(update.data.masters.is_empty());
    let summary = p.review_summary(update.revision).unwrap();
    assert!(summary.project_settings);
    assert_eq!(
        summary.script_changes,
        vec!["gamemasterstudio/scripts/a.json"]
    );
    let review = p
        .review_master(update.revision, "(Project Settings)")
        .unwrap();
    assert!(review.before.unwrap().masters.is_empty());
    assert!(review.after.masters.is_empty());
    assert!(matches!(
        review.changes[0],
        gamemasterstudio_core::project::SemanticChange::ProjectConfig { .. }
    ));
}
