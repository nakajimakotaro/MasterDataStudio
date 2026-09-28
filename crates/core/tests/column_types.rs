use gamemasterstudio_core::{
    config::{ColumnType, ProjectConfig, CONFIG_PATH},
    project::{git, CellEdit, Operation, Project, SemanticChange},
};
use std::fs;

fn apply(project: &mut Project, operation: Operation) {
    project
        .apply(operation, project.snapshot().revision)
        .unwrap();
}

fn set_type(column: &str, column_type: ColumnType) -> Operation {
    Operation::SetColumnTypes {
        master_id: "enemy".into(),
        column_types: [(column.into(), column_type)].into(),
    }
}

#[test]
fn temporal_types_have_distinct_external_names() {
    for (kind, name) in [
        (ColumnType::Date, "date"),
        (ColumnType::Time, "time"),
        (ColumnType::DateTime, "datetime"),
    ] {
        assert_eq!(serde_json::to_value(kind).unwrap(), name);
        let operation: Operation = serde_json::from_value(serde_json::json!({
            "type": "setColumnTypes", "masterId": "enemy", "columnTypes": { "hp": name }
        }))
        .unwrap();
        assert!(
            matches!(operation, Operation::SetColumnTypes { column_types, .. } if column_types["hp"] == kind)
        );
    }
}

fn fixture() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::initialize(dir.path()).unwrap();
    project.switch_branch("work", true).unwrap();
    project
        .set_identity("Tester", "tester@example.com")
        .unwrap();
    git(dir.path(), &["config", "commit.gpgsign", "false"]).unwrap();
    apply(
        &mut project,
        Operation::CreateMaster {
            master_id: "enemy".into(),
            primary_key: vec!["id".into()],
            columns: vec!["id".into(), "hp".into(), "power".into()],
        },
    );
    apply(
        &mut project,
        Operation::CreateRows {
            master_id: "enemy".into(),
            rows: vec![vec!["001".into(), "abc".into(), "NaN".into()]],
        },
    );
    apply(&mut project, set_type("hp", ColumnType::String));
    project.commit("初期データ", false).unwrap();
    (dir, project)
}

#[test]
fn omitted_types_default_to_integer_and_explicit_strings_survive_reopening() {
    let (dir, mut project) = fixture();
    let def = &project.snapshot().data.config.masters["enemy"];
    assert_eq!(def.column_type("id"), ColumnType::Integer);
    assert_eq!(def.column_type("power"), ColumnType::Integer);
    apply(
        &mut project,
        Operation::AddColumn {
            master_id: "enemy".into(),
            name: "extra".into(),
        },
    );
    assert_eq!(
        project.snapshot().data.config.masters["enemy"].column_type("extra"),
        ColumnType::Integer
    );
    apply(&mut project, set_type("extra", ColumnType::String));
    let reopened = Project::open(dir.path()).unwrap().snapshot();
    assert_eq!(
        reopened.data.config.masters["enemy"].column_type("extra"),
        ColumnType::String
    );
    assert!(fs::read_to_string(dir.path().join(CONFIG_PATH))
        .unwrap()
        .contains("extra: string"));
    apply(&mut project, set_type("extra", ColumnType::Integer));
    let reopened = Project::open(dir.path()).unwrap().snapshot();
    assert_eq!(
        reopened.data.config.masters["enemy"].column_type("extra"),
        ColumnType::Integer
    );
    assert!(!reopened.data.config.masters["enemy"]
        .column_types
        .contains_key("extra"));
}

#[test]
fn multiple_types_are_saved_together_and_invalid_batches_leave_files_unchanged() {
    let (dir, mut project) = fixture();
    apply(&mut project, set_type("id", ColumnType::Integer));
    let revision = project.snapshot().revision;
    let operation: Operation = serde_json::from_value(serde_json::json!({
        "type": "setColumnTypes", "masterId": "enemy",
        "columnTypes": { "hp": "integer", "power": "decimal" }
    }))
    .unwrap();
    project.apply(operation, revision).unwrap();
    assert_eq!(project.snapshot().revision, revision + 1);
    let config = ProjectConfig::parse(&fs::read(dir.path().join(CONFIG_PATH)).unwrap()).unwrap();
    assert_eq!(
        config.masters["enemy"].column_type("id"),
        ColumnType::Integer
    );
    assert_eq!(
        config.masters["enemy"].column_type("hp"),
        ColumnType::Integer
    );
    assert_eq!(
        config.masters["enemy"].column_type("power"),
        ColumnType::Decimal
    );

    let saved_config = fs::read(dir.path().join(CONFIG_PATH)).unwrap();
    let saved_csv = fs::read(dir.path().join("masters/enemy.csv")).unwrap();
    let revision = project.snapshot().revision;
    let invalid: Operation = serde_json::from_value(serde_json::json!({
        "type": "setColumnTypes", "masterId": "enemy",
        "columnTypes": { "hp": "boolean", "missing": "date" }
    }))
    .unwrap();
    assert!(project.apply(invalid, revision).is_err());
    assert_eq!(project.snapshot().revision, revision);
    assert_eq!(
        fs::read(dir.path().join(CONFIG_PATH)).unwrap(),
        saved_config
    );
    assert_eq!(
        fs::read(dir.path().join("masters/enemy.csv")).unwrap(),
        saved_csv
    );
    assert_eq!(
        project.snapshot().data.config.masters["enemy"].column_type("hp"),
        ColumnType::Integer
    );
}

#[test]
fn types_round_trip_without_rewriting_invalid_csv_and_can_be_committed() {
    let (dir, mut project) = fixture();
    let original = fs::read(dir.path().join("masters/enemy.csv")).unwrap();
    for kind in [
        ColumnType::Integer,
        ColumnType::Decimal,
        ColumnType::Boolean,
        ColumnType::Date,
        ColumnType::Time,
        ColumnType::DateTime,
        ColumnType::String,
    ] {
        apply(&mut project, set_type("hp", kind));
        let config =
            ProjectConfig::parse(&fs::read(dir.path().join(CONFIG_PATH)).unwrap()).unwrap();
        assert_eq!(config.masters["enemy"].column_type("hp"), kind);
        assert_eq!(
            fs::read(dir.path().join("masters/enemy.csv")).unwrap(),
            original
        );
        project.commit("カラムの型を変更", false).unwrap();
    }
    apply(&mut project, set_type("id", ColumnType::Integer));
    apply(&mut project, set_type("hp", ColumnType::Integer));
    apply(&mut project, set_type("power", ColumnType::Decimal));
    apply(
        &mut project,
        Operation::EditCells {
            master_id: "enemy".into(),
            edits: vec![CellEdit {
                primary_key: vec!["001".into()],
                column: "hp".into(),
                value: "still invalid".into(),
            }],
        },
    );
    project.commit("型エラーを含むデータを保存", false).unwrap();
    let reopened = Project::open(dir.path()).unwrap().full_snapshot();
    assert_eq!(
        reopened.data.config.masters["enemy"].column_type("hp"),
        ColumnType::Integer
    );
    assert_eq!(
        reopened.data.masters["enemy"]
            .data
            .as_ref()
            .unwrap()
            .table
            .rows[0],
        vec!["001", "still invalid", "NaN"]
    );
    let yaml = git(dir.path(), &["show", "HEAD:gamemasterstudio/project.yaml"]).unwrap();
    assert!(yaml.contains("columnTypes:") && !yaml.contains("hp:"));
    let oid = git(dir.path(), &["rev-parse", "HEAD"]).unwrap();
    assert!(project
        .history_detail(&oid)
        .unwrap()
        .changes
        .iter()
        .any(|c| matches!(c, SemanticChange::ColumnType { column, .. } if column == "hp")));
}

#[test]
fn large_numbers_survive_creation_editing_csv_commit_and_reopening_exactly() {
    let (dir, mut project) = fixture();
    apply(&mut project, set_type("hp", ColumnType::Integer));
    apply(&mut project, set_type("power", ColumnType::Decimal));
    let two_to_100 = (1_u128 << 100).to_string();
    let values = [
        two_to_100.clone(),
        ((1_u128 << 100) + 1).to_string(),
        format!("-{two_to_100}"),
        "9007199254740993".into(),
        "9".repeat(400),
    ];
    let rows: Vec<Vec<String>> = values
        .iter()
        .enumerate()
        .map(|(i, value)| vec![format!("large-{i}"), value.clone(), format!("{value}.125")])
        .collect();
    apply(
        &mut project,
        Operation::CreateRows {
            master_id: "enemy".into(),
            rows: rows.clone(),
        },
    );
    for row in &rows {
        apply(
            &mut project,
            Operation::EditCells {
                master_id: "enemy".into(),
                edits: vec![
                    CellEdit {
                        primary_key: vec!["001".into()],
                        column: "hp".into(),
                        value: row[1].clone(),
                    },
                    CellEdit {
                        primary_key: vec!["001".into()],
                        column: "power".into(),
                        value: row[2].clone(),
                    },
                ],
            },
        );
        let mut expected_rows = vec![vec!["001".into(), row[1].clone(), row[2].clone()]];
        expected_rows.extend(rows.clone());
        let expected_csv = format!(
            "id,hp,power\n{}\n",
            expected_rows
                .iter()
                .map(|r| r.join(","))
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("masters/enemy.csv")).unwrap(),
            expected_csv
        );
        let reopened = Project::open(dir.path()).unwrap().full_snapshot();
        assert_eq!(
            reopened.data.masters["enemy"]
                .data
                .as_ref()
                .unwrap()
                .table
                .rows,
            expected_rows
        );
        project.commit("大きな数値をそのまま保存", false).unwrap();
        assert_eq!(
            git(dir.path(), &["show", "HEAD:masters/enemy.csv"]).unwrap(),
            expected_csv.trim_end()
        );
    }
}

#[test]
fn type_revert_preserves_unrelated_cell_edits_and_column_revert_restores_type() {
    let (_dir, mut project) = fixture();
    apply(&mut project, set_type("hp", ColumnType::Integer));
    apply(
        &mut project,
        Operation::EditCells {
            master_id: "enemy".into(),
            edits: vec![CellEdit {
                primary_key: vec!["001".into()],
                column: "power".into(),
                value: "changed".into(),
            }],
        },
    );
    let change = project
        .semantic_diff()
        .unwrap()
        .into_iter()
        .find(|c| matches!(c, SemanticChange::ColumnType { .. }))
        .unwrap();
    project
        .revert_change(change, project.snapshot().revision)
        .unwrap();
    let snapshot = project.full_snapshot();
    assert_eq!(
        snapshot.data.config.masters["enemy"].column_type("hp"),
        ColumnType::String
    );
    assert_eq!(
        snapshot.data.masters["enemy"]
            .data
            .as_ref()
            .unwrap()
            .table
            .rows[0][2],
        "changed"
    );
    apply(&mut project, set_type("hp", ColumnType::Date));
    project.commit("日付型を設定", false).unwrap();
    apply(
        &mut project,
        Operation::DeleteColumn {
            master_id: "enemy".into(),
            name: "hp".into(),
        },
    );
    assert!(!project.snapshot().data.config.masters["enemy"]
        .column_types
        .contains_key("hp"));
    let change = project
        .semantic_diff()
        .unwrap()
        .into_iter()
        .find(|c| matches!(c, SemanticChange::DeletedColumn { .. }))
        .unwrap();
    project
        .revert_change(change, project.snapshot().revision)
        .unwrap();
    let snapshot = project.full_snapshot();
    assert_eq!(
        snapshot.data.config.masters["enemy"].column_type("hp"),
        ColumnType::Date
    );
    let table = &snapshot.data.masters["enemy"].data.as_ref().unwrap().table;
    let index = table.columns.iter().position(|c| c == "hp").unwrap();
    assert_eq!(table.rows[0][index], "abc");
}

#[test]
fn missing_columns_and_unknown_types_are_rejected_but_empty_masters_preserve_types_on_rekey() {
    let (_dir, mut project) = fixture();
    assert!(project
        .apply(
            set_type("missing", ColumnType::Integer),
            project.snapshot().revision
        )
        .is_err());
    assert!(serde_json::from_str::<Operation>(
        r#"{"type":"setColumnTypes","masterId":"enemy","columnTypes":{"hp":"unknown"}}"#
    )
    .is_err());
    apply(
        &mut project,
        Operation::DeleteRows {
            master_id: "enemy".into(),
            primary_keys: vec![vec!["001".into()]],
        },
    );
    apply(&mut project, set_type("hp", ColumnType::Decimal));
    apply(
        &mut project,
        Operation::ConfigureMaster {
            master_id: "enemy".into(),
            primary_key: vec!["power".into()],
        },
    );
    assert_eq!(
        project.snapshot().data.config.masters["enemy"].column_type("hp"),
        ColumnType::Decimal
    );
    apply(
        &mut project,
        Operation::AddColumn {
            master_id: "enemy".into(),
            name: "extra".into(),
        },
    );
    apply(&mut project, set_type("extra", ColumnType::Boolean));
    let change = project
        .semantic_diff()
        .unwrap()
        .into_iter()
        .find(|c| matches!(c, SemanticChange::MasterDefinition { .. }))
        .unwrap();
    project
        .revert_change(change, project.snapshot().revision)
        .unwrap();
    assert!(!project.snapshot().data.config.masters["enemy"]
        .column_types
        .contains_key("extra"));
}
