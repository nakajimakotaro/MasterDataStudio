use gamemasterstudio_core::project::{git, Project};
use std::fs;

fn source_path(source: &tempfile::TempDir) -> String {
    source.path().to_str().unwrap().replace('\\', "/")
}

#[test]
fn clones_project_with_origin_and_upstream() {
    let source = tempfile::tempdir().unwrap();
    Project::initialize(source.path()).unwrap();
    git(source.path(), &["add", "."]).unwrap();
    git(
        source.path(),
        &[
            "-c",
            "user.name=Tester",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "initial",
        ],
    )
    .unwrap();
    let target = tempfile::tempdir().unwrap();
    let project = Project::clone_repository(&source_path(&source), target.path()).unwrap();
    let snapshot = project.snapshot();
    assert_eq!(snapshot.git.branch, "main");
    assert!(snapshot.git.remotes.contains(&"origin".to_string()));
    assert_eq!(snapshot.git.upstream.as_deref(), Some("origin/main"));
    assert!(snapshot.changes.is_empty());
}

#[test]
fn rejects_nonempty_destination_without_changing_files() {
    let target = tempfile::tempdir().unwrap();
    fs::write(target.path().join("keep.txt"), "keep").unwrap();
    assert!(Project::clone_repository("https://example.com/project.git", target.path()).is_err());
    assert_eq!(
        fs::read_to_string(target.path().join("keep.txt")).unwrap(),
        "keep"
    );
    assert!(!target.path().join(".git").exists());
}

#[test]
fn rejects_empty_url_and_git_options() {
    let target = tempfile::tempdir().unwrap();
    for url in ["", "  ", "--help", "url\nother"] {
        assert!(Project::clone_repository(url, target.path()).is_err());
    }
    assert_eq!(fs::read_dir(target.path()).unwrap().count(), 0);
}

#[test]
fn retains_clone_when_repository_is_not_a_project() {
    let source = tempfile::tempdir().unwrap();
    git(source.path(), &["init", "-b", "main"]).unwrap();
    let target = tempfile::tempdir().unwrap();
    let error = Project::clone_repository(&source_path(&source), target.path())
        .err()
        .unwrap();
    assert!(error.contains("クローンは完了しました"));
    assert!(target.path().join(".git").is_dir());
}
