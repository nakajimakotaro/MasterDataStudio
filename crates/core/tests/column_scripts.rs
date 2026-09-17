use gamemasterstudio_core::{
    project::{git, CellEdit, Master, Operation, Project},
    scripts::{CalculatedCell, Scripts},
};
use std::fs;

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| (*s).into()).collect()
}
fn edit(key: &[&str], column: &str, value: &str) -> CellEdit {
    CellEdit {
        primary_key: strings(key),
        column: column.into(),
        value: value.into(),
    }
}
fn set_script(column: &str, script: Option<&str>) -> Operation {
    Operation::SetScript {
        master_id: "enemy".into(),
        column: column.into(),
        script: script.map(str::to_owned),
    }
}
fn master(project: &Project) -> Master {
    project.snapshot().data.masters["enemy"]
        .data
        .clone()
        .unwrap()
}
fn apply(project: &mut Project, op: Operation) {
    let revision = project.snapshot().revision;
    let preview = project.preview(op.clone(), revision).unwrap();
    let calculated = preview
        .targets
        .iter()
        .map(|t| {
            let m = preview.data.masters[&t.master_id].data.as_ref().unwrap();
            let def = &preview.data.config.masters[&t.master_id];
            let row = &m.table.rows[m.table.row_index(&t.primary_key, def).unwrap()];
            let input: i32 = row[2].parse().unwrap_or(0);
            CalculatedCell {
                master_id: t.master_id.clone(),
                edit: CellEdit {
                    primary_key: t.primary_key.clone(),
                    column: t.column.clone(),
                    value: (input * if t.column == "power" { 2 } else { 3 }).to_string(),
                },
            }
        })
        .collect();
    project.apply_calculated(op, calculated, revision).unwrap();
}
fn fixture() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    let mut p = Project::initialize(dir.path()).unwrap();
    p.switch_branch("work", true).unwrap();
    p.set_identity("Script Tester", "script@example.com")
        .unwrap();
    git(dir.path(), &["config", "commit.gpgsign", "false"]).unwrap();
    apply(
        &mut p,
        Operation::CreateMaster {
            master_id: "enemy".into(),
            path: "enemy.csv".into(),
            primary_key: strings(&["id", "wave"]),
            columns: strings(&["id", "wave", "attack", "power", "score"]),
        },
    );
    apply(
        &mut p,
        Operation::CreateRows {
            master_id: "enemy".into(),
            rows: vec![
                strings(&["1", "a", "20", "", ""]),
                strings(&["2", "b", "30", "", ""]),
            ],
        },
    );
    p.commit("initial", false).unwrap();
    (dir, p)
}

#[test]
fn preparation_results_and_history_are_one_atomic_operation() {
    let (dir, mut p) = fixture();
    let revision = p.snapshot().revision;
    let initial = master(&p);
    let op = set_script("power", Some("return Number(row.attack) * 2;"));
    let preview = p.preview(op.clone(), revision).unwrap();
    assert_eq!(preview.targets.len(), 2);
    assert_eq!(master(&p), initial);
    assert!(!dir
        .path()
        .join("gamemasterstudio/scripts/enemy.json")
        .exists());
    assert!(p.apply(op.clone(), revision).is_err());
    assert!(p
        .apply_calculated(
            op.clone(),
            vec![CalculatedCell {
                master_id: "enemy".into(),
                edit: edit(&["1", "a"], "attack", "hijack")
            }],
            revision
        )
        .is_err());
    assert!(p
        .apply_calculated(
            op.clone(),
            vec![CalculatedCell {
                master_id: "enemy".into(),
                edit: edit(&["1", "a"], "power", "40")
            }],
            revision
        )
        .is_err());
    assert_eq!(p.snapshot().revision, revision);
    assert_eq!(master(&p), initial);
    apply(&mut p, op);
    let calculated = master(&p);
    assert_eq!(calculated.table.rows[0][3], "40");
    p.undo(p.snapshot().revision).unwrap();
    assert_eq!(master(&p), initial);
    assert!(!dir
        .path()
        .join("gamemasterstudio/scripts/enemy.json")
        .exists());
    p.redo(p.snapshot().revision).unwrap();
    assert_eq!(master(&p), calculated);
    assert!(p
        .preview(set_script("power", Some("return 0;")), revision)
        .is_err());
}

#[test]
fn mixed_paste_overrides_and_recalculation_share_undo() {
    let (_dir, mut p) = fixture();
    apply(&mut p, set_script("power", Some("return 0;")));
    apply(&mut p, set_script("score", Some("return 0;")));
    let before = master(&p);
    let op = Operation::EditCells {
        master_id: "enemy".into(),
        edits: vec![
            edit(&["1", "a"], "attack", "50"),
            edit(&["1", "a"], "power", "777"),
            edit(&["1", "a"], "attack", "60"),
        ],
    };
    let preview = p.preview(op.clone(), p.snapshot().revision).unwrap();
    assert_eq!(preview.targets.len(), 1);
    assert_eq!(preview.targets[0].column, "score");
    apply(&mut p, op);
    assert_eq!(
        master(&p).table.rows[0],
        strings(&["1", "a", "60", "777", "180"])
    );
    assert_eq!(
        master(&p).scripts.columns[0].overrides,
        vec![strings(&["1", "a"])]
    );
    let after = master(&p);
    p.undo(p.snapshot().revision).unwrap();
    assert_eq!(master(&p), before);
    p.redo(p.snapshot().revision).unwrap();
    assert_eq!(master(&p), after);
    apply(&mut p, set_script("power", Some("return 9;")));
    assert_eq!(master(&p).table.rows[0][3], "777");
    apply(
        &mut p,
        Operation::RemoveOverride {
            master_id: "enemy".into(),
            primary_key: strings(&["1", "a"]),
            column: "power".into(),
        },
    );
    assert_eq!(master(&p).table.rows[0][3], "120");
    assert!(master(&p).scripts.columns[0].overrides.is_empty());
}

#[test]
fn direct_override_does_not_recalculate_other_scripts_and_pk_batch_is_rejected() {
    let (_dir, mut p) = fixture();
    apply(&mut p, set_script("power", Some("return 0;")));
    let op = Operation::EditCells {
        master_id: "enemy".into(),
        edits: vec![edit(&["1", "a"], "power", "40")],
    };
    assert!(p
        .preview(op.clone(), p.snapshot().revision)
        .unwrap()
        .targets
        .is_empty());
    apply(&mut p, op);
    assert_eq!(master(&p).scripts.columns[0].overrides.len(), 1);
    let before = master(&p);
    assert!(p
        .apply(
            Operation::EditCells {
                master_id: "enemy".into(),
                edits: vec![
                    edit(&["1", "a"], "attack", "999"),
                    edit(&["1", "a"], "id", "new")
                ]
            },
            p.snapshot().revision
        )
        .is_err());
    assert_eq!(master(&p), before);
    for column in ["id", "missing"] {
        assert!(p
            .preview(set_script(column, Some("return 0;")), p.snapshot().revision)
            .is_err());
    }
}

#[test]
fn create_duplicate_delete_and_remove_script_preserve_metadata_rules() {
    let (dir, mut p) = fixture();
    apply(&mut p, set_script("power", Some("return 0;")));
    apply(
        &mut p,
        Operation::EditCells {
            master_id: "enemy".into(),
            edits: vec![edit(&["1", "a"], "power", "999")],
        },
    );
    let op = Operation::AddRow {
        master_id: "enemy".into(),
        primary_key: strings(&["3", "c"]),
        duplicate_from: Some(strings(&["1", "a"])),
    };
    let preview = p.preview(op.clone(), p.snapshot().revision).unwrap();
    assert_eq!(
        preview.data.masters["enemy"]
            .data
            .as_ref()
            .unwrap()
            .table
            .rows[2][3],
        ""
    );
    assert_eq!(preview.targets.len(), 1);
    apply(&mut p, op);
    assert_eq!(master(&p).table.rows[2][3], "40");
    assert_eq!(master(&p).scripts.columns[0].overrides.len(), 1);
    apply(
        &mut p,
        Operation::DeleteRows {
            master_id: "enemy".into(),
            primary_keys: vec![strings(&["1", "a"])],
        },
    );
    assert!(master(&p).scripts.columns[0].overrides.is_empty());
    p.undo(p.snapshot().revision).unwrap();
    assert_eq!(master(&p).scripts.columns[0].overrides.len(), 1);
    let table = master(&p).table;
    apply(&mut p, set_script("power", None));
    assert_eq!(master(&p).table, table);
    assert!(!dir
        .path()
        .join("gamemasterstudio/scripts/enemy.json")
        .exists());
    p.undo(p.snapshot().revision).unwrap();
    apply(
        &mut p,
        Operation::DeleteColumn {
            master_id: "enemy".into(),
            name: "power".into(),
        },
    );
    assert!(master(&p).scripts.columns.is_empty());
    assert!(!dir
        .path()
        .join("gamemasterstudio/scripts/enemy.json")
        .exists());
    p.undo(p.snapshot().revision).unwrap();
    assert_eq!(master(&p).scripts.columns[0].overrides.len(), 1);
}

#[test]
fn safe_mode_keeps_values_and_all_operations_skip_execution() {
    let (_dir, mut p) = fixture();
    p.safe_mode = true;
    apply(&mut p, set_script("power", Some("while (true) {}")));
    assert_eq!(master(&p).table.rows[0][3], "");
    apply(
        &mut p,
        Operation::EditCells {
            master_id: "enemy".into(),
            edits: vec![
                edit(&["1", "a"], "power", "manual"),
                edit(&["1", "a"], "attack", "99"),
            ],
        },
    );
    apply(
        &mut p,
        Operation::RemoveOverride {
            master_id: "enemy".into(),
            primary_key: strings(&["1", "a"]),
            column: "power".into(),
        },
    );
    assert_eq!(master(&p).table.rows[0][3], "manual");
    apply(&mut p, set_script("power", Some("return (")));
    apply(
        &mut p,
        Operation::CreateRows {
            master_id: "enemy".into(),
            rows: vec![strings(&["3", "c", "2", "do not copy", ""])],
        },
    );
    assert_eq!(master(&p).table.rows[2][3], "");
    p.commit("recovery", false).unwrap();
    assert!(p.snapshot().safe_mode);
    p.switch_branch("recovered", true).unwrap();
    assert!(p.snapshot().safe_mode);
}

#[test]
fn canonical_metadata_validates_and_safe_mode_can_repair_invalid_structure() {
    let (dir, mut p) = fixture();
    p.safe_mode = true;
    let mut metadata: Scripts = serde_json::from_str(r#"{"version":1,"columns":[{"column":"score","script":"return 1;\r\n","overrides":[["2","b"],["1","a"]]},{"column":"power","script":"return 2;","overrides":[]}]}"#).unwrap();
    let m = master(&p);
    metadata
        .validate(&m.table, &p.snapshot().data.config.masters["enemy"])
        .unwrap();
    assert_eq!(metadata.columns[0].column, "power");
    assert_eq!(metadata.columns[1].overrides[0], strings(&["1", "a"]));
    apply(
        &mut p,
        Operation::ReplaceScripts {
            master_id: "enemy".into(),
            scripts: metadata.clone(),
        },
    );
    let path = dir.path().join("gamemasterstudio/scripts/enemy.json");
    let bytes = fs::read(&path).unwrap();
    assert_eq!(bytes.last(), Some(&b'\n'));
    assert!(!bytes.contains(&b'\r'));
    assert!(String::from_utf8(bytes)
        .unwrap()
        .starts_with("{\n  \"version\": 1,"));
    let mut invalid = metadata.clone();
    invalid.columns[1].overrides.push(strings(&["1", "a"]));
    assert!(invalid
        .validate(&m.table, &p.snapshot().data.config.masters["enemy"])
        .is_err());
    fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    let mut opened = Project::open(dir.path()).unwrap();
    assert!(master(&opened).script_error.is_some());
    assert!(opened
        .preview(
            Operation::RecalculateScripts {
                master_id: "enemy".into()
            },
            opened.snapshot().revision
        )
        .is_err());
    opened.safe_mode = true;
    apply(
        &mut opened,
        Operation::ReplaceScripts {
            master_id: "enemy".into(),
            scripts: metadata,
        },
    );
    assert!(master(&opened).script_error.is_none());
    fs::write(path, b"{").unwrap();
    assert!(
        Project::open(dir.path()).unwrap().snapshot().data.masters["enemy"]
            .error
            .as_ref()
            .unwrap()
            .contains("Script JSON")
    );
}

#[test]
fn metadata_only_changes_commit_and_untracked_metadata_blocks_branch_switch() {
    let (dir, mut p) = fixture();
    p.safe_mode = true;
    apply(&mut p, set_script("power", Some("return 1;")));
    assert!(p.snapshot().changes.is_empty());
    assert_eq!(
        p.snapshot().script_changes,
        vec!["gamemasterstudio/scripts/enemy.json"]
    );
    assert!(p.switch_branch("blocked", true).is_err());
    p.commit("script only", false).unwrap();
    assert!(git(
        dir.path(),
        &["show", "HEAD:gamemasterstudio/scripts/enemy.json"]
    )
    .unwrap()
    .contains("return 1;"));
    assert!(p.snapshot().script_changes.is_empty());
    apply(&mut p, set_script("power", None));
    p.commit("remove script", false).unwrap();
    assert!(git(
        dir.path(),
        &["show", "HEAD:gamemasterstudio/scripts/enemy.json"]
    )
    .is_err());
}

#[test]
fn failed_metadata_write_rolls_back_csv_state_and_history() {
    let (dir, mut p) = fixture();
    let before = master(&p);
    let revision = p.snapshot().revision;
    let csv = fs::read(dir.path().join("enemy.csv")).unwrap();
    fs::write(
        dir.path().join("gamemasterstudio/scripts"),
        "not a directory",
    )
    .unwrap();
    let op = set_script("power", Some("return 1;"));
    let cells = vec![
        CalculatedCell {
            master_id: "enemy".into(),
            edit: edit(&["1", "a"], "power", "40"),
        },
        CalculatedCell {
            master_id: "enemy".into(),
            edit: edit(&["2", "b"], "power", "60"),
        },
    ];
    assert!(p.apply_calculated(op, cells, revision).is_err());
    assert_eq!(master(&p), before);
    assert_eq!(p.snapshot().revision, revision);
    assert_eq!(fs::read(dir.path().join("enemy.csv")).unwrap(), csv);
}

#[test]
fn one_sided_script_metadata_survives_csv_semantic_merge() {
    let (dir, mut p) = fixture();
    p.safe_mode = true;
    apply(&mut p, set_script("power", Some("return 1;")));
    p.commit("base script", false).unwrap();
    p.switch_branch("incoming", true).unwrap();
    apply(&mut p, set_script("power", Some("return 2;")));
    apply(
        &mut p,
        Operation::EditCells {
            master_id: "enemy".into(),
            edits: vec![edit(&["1", "a"], "score", "8")],
        },
    );
    p.commit("incoming metadata and cell", false).unwrap();
    p.switch_branch("work", false).unwrap();
    apply(
        &mut p,
        Operation::EditCells {
            master_id: "enemy".into(),
            edits: vec![edit(&["1", "a"], "attack", "99")],
        },
    );
    p.commit("our cell", false).unwrap();
    p.merge_branch("incoming").unwrap();
    assert!(p.snapshot().merge.is_none());
    assert_eq!(master(&p).scripts.columns[0].script, "return 2;");
    assert_eq!(master(&p).table.rows[0][2], "99");
    assert_eq!(master(&p).table.rows[0][4], "8");
    assert!(git(
        dir.path(),
        &["show", "HEAD:gamemasterstudio/scripts/enemy.json"]
    )
    .unwrap()
    .contains("return 2;"));
}

#[test]
fn conflicting_script_metadata_aborts_merge_without_losing_changes() {
    let (dir, mut p) = fixture();
    p.safe_mode = true;
    apply(&mut p, set_script("power", Some("return 1;")));
    p.commit("base script", false).unwrap();
    p.switch_branch("incoming", true).unwrap();
    apply(&mut p, set_script("power", Some("return 2;")));
    p.commit("incoming script", false).unwrap();
    p.switch_branch("work", false).unwrap();
    apply(&mut p, set_script("power", Some("return 3;")));
    p.commit("our script", false).unwrap();
    let head = git(dir.path(), &["rev-parse", "HEAD"]).unwrap();
    let before = master(&p);
    assert!(p
        .merge_branch("incoming")
        .err()
        .unwrap()
        .contains("Script metadata"));
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]).unwrap(), head);
    assert!(!p.snapshot().git.merge_in_progress);
    assert_eq!(master(&p), before);
}

#[test]
fn reverting_an_input_recalculates_in_the_same_undo_operation() {
    use gamemasterstudio_core::project::SemanticChange;
    let (_dir, mut p) = fixture();
    apply(&mut p, set_script("power", Some("return 0;")));
    p.commit("script", false).unwrap();
    apply(
        &mut p,
        Operation::EditCells {
            master_id: "enemy".into(),
            edits: vec![edit(&["1", "a"], "attack", "50")],
        },
    );
    let before = master(&p);
    let change = p
        .snapshot()
        .changes
        .into_iter()
        .find(|c| matches!(c, SemanticChange::Cell { column, .. } if column == "attack"))
        .unwrap();
    apply(&mut p, Operation::RevertChange { change });
    assert_eq!(master(&p).table.rows[0][2], "20");
    assert_eq!(master(&p).table.rows[0][3], "40");
    p.undo(p.snapshot().revision).unwrap();
    assert_eq!(master(&p), before);
}

#[test]
fn configuring_an_empty_master_cannot_turn_a_script_column_into_a_key() {
    let (_dir, mut p) = fixture();
    apply(
        &mut p,
        Operation::DeleteRows {
            master_id: "enemy".into(),
            primary_keys: vec![strings(&["1", "a"]), strings(&["2", "b"])],
        },
    );
    apply(&mut p, set_script("power", Some("return 0;")));
    let before = p.snapshot().data;
    assert!(p
        .apply(
            Operation::ConfigureMaster {
                master_id: "enemy".into(),
                path: "enemy.csv".into(),
                primary_key: strings(&["power"])
            },
            p.snapshot().revision
        )
        .is_err());
    assert_eq!(p.snapshot().data, before);
}
