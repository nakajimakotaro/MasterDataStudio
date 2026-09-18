use gamemasterstudio_core::project::{git, Operation, Project, SemanticChange};
use std::fs;

fn apply(project: &mut Project, operation: Operation) {
    project
        .apply(operation, project.snapshot().revision)
        .unwrap();
}

fn fixture() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::initialize(dir.path()).unwrap();
    project.switch_branch("setup", true).unwrap();
    project
        .set_identity("Tester", "tester@example.com")
        .unwrap();
    apply(
        &mut project,
        Operation::CreateMaster {
            master_id: "enemy".into(),
            path: "masters/enemy.csv".into(),
            primary_key: vec!["id".into()],
            columns: vec!["id".into(), "hp".into()],
        },
    );
    git(
        dir.path(),
        &["add", "gamemasterstudio/project.yaml", "masters/enemy.csv"],
    )
    .unwrap();
    git(dir.path(), &["commit", "-m", "initial"]).unwrap();
    git(dir.path(), &["branch", "main"]).unwrap();
    project.switch_branch("feature/data", true).unwrap();
    (dir, project)
}

#[test]
fn semantic_diff_and_individual_revert_use_primary_key() {
    let (_dir, mut project) = fixture();
    apply(
        &mut project,
        Operation::AddRow {
            master_id: "enemy".into(),
            primary_key: vec!["10".into()],
            duplicate_from: None,
        },
    );
    apply(
        &mut project,
        Operation::EditCells {
            master_id: "enemy".into(),
            edits: vec![gamemasterstudio_core::project::CellEdit {
                primary_key: vec!["10".into()],
                column: "hp".into(),
                value: "120".into(),
            }],
        },
    );
    let changes = project.semantic_diff().unwrap();
    assert!(changes.iter().any(
        |c| matches!(c, SemanticChange::AddedRow{primary_key,..} if primary_key == &vec!["10"])
    ));
    let added = changes
        .into_iter()
        .find(|c| matches!(c, SemanticChange::AddedRow { .. }))
        .unwrap();
    project
        .revert_change(added, project.snapshot().revision)
        .unwrap();
    assert!(project.semantic_diff().unwrap().is_empty());
}

#[test]
fn commit_stages_only_managed_files() {
    let (dir, mut project) = fixture();
    fs::write(dir.path().join("README.txt"), "unmanaged").unwrap();
    git(dir.path(), &["add", "README.txt"]).unwrap();
    git(dir.path(), &["commit", "-m", "readme"]).unwrap();
    fs::write(dir.path().join("README.txt"), "do not commit").unwrap();
    apply(
        &mut project,
        Operation::AddRow {
            master_id: "enemy".into(),
            primary_key: vec!["1".into()],
            duplicate_from: None,
        },
    );
    project.commit("data update", false).unwrap();
    assert_eq!(
        git(dir.path(), &["show", "HEAD:README.txt"]).unwrap(),
        "unmanaged"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("README.txt")).unwrap(),
        "do not commit"
    );
    assert!(git(dir.path(), &["status", "--porcelain", "README.txt"])
        .unwrap()
        .contains("README.txt"));
}

#[test]
fn protected_branch_blocks_edits_after_initial_commit() {
    let (dir, mut project) = fixture();
    project.switch_branch("main", false).unwrap();
    assert!(project
        .apply(
            Operation::AddColumn {
                master_id: "enemy".into(),
                name: "name".into()
            },
            project.snapshot().revision
        )
        .is_err());
    assert!(project.snapshot().git.protected);
    assert_eq!(
        git(dir.path(), &["branch", "--show-current"]).unwrap(),
        "main"
    );
}

#[test]
fn change_review_preserves_deleted_values_and_is_read_only() {
    let (dir, mut project) = fixture();
    apply(
        &mut project,
        Operation::AddRow {
            master_id: "enemy".into(),
            primary_key: vec!["10".into()],
            duplicate_from: None,
        },
    );
    apply(
        &mut project,
        Operation::EditCells {
            master_id: "enemy".into(),
            edits: vec![gamemasterstudio_core::project::CellEdit {
                primary_key: vec!["10".into()],
                column: "hp".into(),
                value: "120".into(),
            }],
        },
    );
    project.commit("seed row", false).unwrap();
    apply(
        &mut project,
        Operation::DeleteColumn {
            master_id: "enemy".into(),
            name: "hp".into(),
        },
    );
    apply(
        &mut project,
        Operation::DeleteRows {
            master_id: "enemy".into(),
            primary_keys: vec![vec!["10".into()]],
        },
    );
    let snapshot = project.full_snapshot();
    let status = git(dir.path(), &["status", "--porcelain"]).unwrap();
    let review = project.change_review(snapshot.revision).unwrap();
    let before = review.before.unwrap();
    let original = before.masters["enemy"].data.as_ref().unwrap();
    assert_eq!(original.table.columns, vec!["id", "hp"]);
    assert_eq!(original.table.rows, vec![vec!["10", "120"]]);
    assert_eq!(review.after, snapshot.data);
    assert!(review
        .changes
        .iter()
        .any(|c| matches!(c, SemanticChange::DeletedRow { .. })));
    assert!(review
        .changes
        .iter()
        .any(|c| matches!(c, SemanticChange::DeletedColumn { .. })));
    assert_eq!(project.snapshot().revision, snapshot.revision);
    assert_eq!(git(dir.path(), &["status", "--porcelain"]).unwrap(), status);
    assert!(project.change_review(snapshot.revision + 1).is_err());

    let deleted_row = review
        .changes
        .into_iter()
        .find(|c| matches!(c, SemanticChange::DeletedRow { .. }))
        .unwrap();
    project
        .revert_change(deleted_row, snapshot.revision)
        .unwrap();
    let updated = project.change_review(project.snapshot().revision).unwrap();
    assert!(!updated
        .changes
        .iter()
        .any(|c| matches!(c, SemanticChange::DeletedRow { .. })));
    assert_eq!(
        updated.after.masters["enemy"]
            .data
            .as_ref()
            .unwrap()
            .table
            .rows,
        vec![vec!["10"]]
    );
    assert!(project.change_review(snapshot.revision).is_err());
}

#[test]
fn change_review_supports_initial_commit_and_protected_branches() {
    let dir = tempfile::tempdir().unwrap();
    let project = Project::initialize(dir.path()).unwrap();
    let review = project.change_review(project.snapshot().revision).unwrap();
    assert!(review.before.is_none());
    assert!(!review.changes.is_empty());

    let (_dir, mut project) = fixture();
    project.switch_branch("main", false).unwrap();
    assert!(project.snapshot().git.protected);
    let review = project.change_review(project.snapshot().revision).unwrap();
    assert!(review.before.is_some());
    assert!(review.changes.is_empty());
}
