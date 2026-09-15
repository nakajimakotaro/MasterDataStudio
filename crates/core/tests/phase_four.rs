use gamemasterstudio_core::{
    comments::{Comment, CommentTarget, Comments, Identity},
    config::{MasterDefinition, ProjectConfig, CONFIG_PATH},
    csv_data::Table,
    merge::{merge_comment, merge_project, Resolution},
    project::{git, Master, MasterEntry, Operation, Project, ProjectData, SemanticChange},
};
use std::{collections::BTreeMap, fs, path::Path};

fn author(name: &str) -> Identity {
    Identity {
        name: name.into(),
        email: format!("{name}@example.com"),
    }
}
fn comment(body: &str, second: u8, name: &str) -> Option<Comment> {
    Some(Comment {
        body: body.into(),
        created_by: author(name),
        updated_by: author(name),
        created_at: "2026-09-15T00:00:00.000Z".into(),
        updated_at: format!("2026-09-15T00:00:{second:02}.000Z"),
    })
}
fn data() -> ProjectData {
    let def = MasterDefinition {
        path: "masters/enemy.csv".into(),
        primary_key: vec!["id".into(), "wave".into()],
    };
    let table = Table::parse(b"id,wave,hp\n1,2,100\n1,10,200\n", &def).unwrap();
    ProjectData {
        config: ProjectConfig {
            masters: BTreeMap::from([("enemy".into(), def)]),
            ..ProjectConfig::default()
        },
        masters: BTreeMap::from([(
            "enemy".into(),
            MasterEntry {
                data: Some(Master {
                    table,
                    comments: Comments::default(),
                }),
                error: None,
            },
        )]),
    }
}
fn comments(data: &mut ProjectData) -> &mut Comments {
    &mut data
        .masters
        .get_mut("enemy")
        .unwrap()
        .data
        .as_mut()
        .unwrap()
        .comments
}
fn apply(p: &mut Project, op: Operation) {
    p.apply(op, p.snapshot().revision).unwrap();
}
fn write(root: &Path, data: &ProjectData) {
    fs::create_dir_all(root.join("gamemasterstudio/comments")).unwrap();
    fs::create_dir_all(root.join("masters")).unwrap();
    fs::write(root.join(CONFIG_PATH), data.config.serialize().unwrap()).unwrap();
    for (id, def) in &data.config.masters {
        let m = data.masters[id].data.as_ref().unwrap();
        fs::write(root.join(&def.path), m.table.serialize(def).unwrap()).unwrap();
        let path = root.join(format!("gamemasterstudio/comments/{id}.json"));
        match m.comments.serialize().unwrap() {
            Some(b) => fs::write(path, b).unwrap(),
            None => {
                let _ = fs::remove_file(path);
            }
        }
    }
}
fn save(root: &Path, message: &str) {
    git(root, &["add", "-A"]).unwrap();
    git(root, &["commit", "--allow-empty", "-m", message]).unwrap();
}
fn repo(b: &ProjectData, a: &ProjectData, c: &ProjectData) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    git(r, &["init", "-b", "work"]).unwrap();
    git(r, &["config", "user.name", "Resolver"]).unwrap();
    git(r, &["config", "user.email", "resolver@example.com"]).unwrap();
    git(r, &["config", "commit.gpgsign", "false"]).unwrap();
    write(r, b);
    save(r, "初期データ");
    git(r, &["branch", "main"]).unwrap();
    git(r, &["switch", "-c", "incoming"]).unwrap();
    write(r, c);
    save(r, "Incoming");
    git(r, &["switch", "work"]).unwrap();
    write(r, a);
    save(r, "Your Branch");
    dir
}

#[test]
fn comment_truth_table_uses_body_not_metadata() {
    let values = [None, Some("a"), Some("b"), Some("c")];
    for b in values {
        for a in values {
            for c in values {
                let (bv, av, cv) = (
                    b.and_then(|v| comment(v, 1, "base")),
                    a.and_then(|v| comment(v, 2, "ours")),
                    c.and_then(|v| comment(v, 3, "theirs")),
                );
                let result = merge_comment(&bv, &av, &cv);
                if a == c || b == c {
                    assert_eq!(
                        result.as_ref().map(|v| v.as_ref().map(|c| c.body.as_str())),
                        Some(a)
                    );
                } else if a == b {
                    assert_eq!(
                        result.as_ref().map(|v| v.as_ref().map(|c| c.body.as_str())),
                        Some(c)
                    );
                } else {
                    assert!(result.is_none());
                }
            }
        }
    }
}
#[test]
fn equal_body_uses_newest_timestamp_and_ours_on_tie() {
    let a = comment("same\nbody", 1, "ours");
    let mut c = comment("same\r\nbody", 2, "theirs");
    assert_eq!(
        merge_comment(&None, &a, &c)
            .unwrap()
            .unwrap()
            .updated_by
            .name,
        "theirs"
    );
    c.as_mut().unwrap().updated_at = "2026-09-15T09:00:01.000+09:00".into();
    assert_eq!(
        merge_comment(&None, &a, &c)
            .unwrap()
            .unwrap()
            .updated_by
            .name,
        "ours"
    );
    assert_eq!(
        merge_comment(&None, &c, &a)
            .unwrap()
            .unwrap()
            .updated_by
            .name,
        "theirs"
    );
}
#[test]
fn distinct_table_row_cell_identities_merge_and_conflicts_are_individual() {
    let b = data();
    let mut a = b.clone();
    let mut c = b.clone();
    comments(&mut a).table = comment("table", 1, "a");
    comments(&mut c).set(
        &CommentTarget::Row {
            primary_key: vec!["1".into(), "10".into()],
        },
        "row",
        &author("b"),
    );
    for (d, body) in [(&mut a, "ours"), (&mut c, "theirs")] {
        comments(d).set(
            &CommentTarget::Cell {
                primary_key: vec!["1".into(), "2".into()],
                column: "hp".into(),
            },
            body,
            &author("writer"),
        );
    }
    let plan = merge_project(&b, &a, &c, &BTreeMap::new()).unwrap();
    assert_eq!(plan.view.remaining, 1);
    let conflict = &plan.view.conflicts[0];
    assert_eq!(conflict.kind, "comment");
    assert_eq!(conflict.primary_key.as_ref().unwrap(), &vec!["1", "2"]);
    assert_eq!(conflict.column.as_deref(), Some("hp"));
    let mut resolved = merge_project(
        &b,
        &a,
        &c,
        &BTreeMap::from([(conflict.id.clone(), Resolution::Theirs)]),
    )
    .unwrap();
    assert_eq!(resolved.view.remaining, 0);
    let m = comments(&mut resolved.data);
    assert!(m.table.is_some());
    assert_eq!(m.rows.len(), 1);
    assert_eq!(m.cells[0].comment.body, "theirs");
}
#[test]
fn deleted_structure_removes_even_conflicting_comments() {
    let mut b = data();
    let target = CommentTarget::Cell {
        primary_key: vec!["1".into(), "2".into()],
        column: "hp".into(),
    };
    comments(&mut b).set(&target, "base", &author("base"));
    let mut a = b.clone();
    let mut c = b.clone();
    let m = a.masters.get_mut("enemy").unwrap().data.as_mut().unwrap();
    m.table.columns.pop();
    for r in &mut m.table.rows {
        r.pop();
    }
    m.comments.cells.clear();
    comments(&mut c).set(&target, "incoming edit", &author("other"));
    let mut plan = merge_project(&b, &a, &c, &BTreeMap::new()).unwrap();
    assert_eq!(plan.view.remaining, 0);
    assert!(comments(&mut plan.data).cells.is_empty());
}
#[test]
fn git_merge_custom_comment_preserves_creator_and_stamps_resolver() {
    let mut b = data();
    comments(&mut b).table = comment("base", 0, "creator");
    let mut a = b.clone();
    let mut c = b.clone();
    comments(&mut a).table.as_mut().unwrap().body = "ours".into();
    comments(&mut c).table.as_mut().unwrap().body = "theirs".into();
    let dir = repo(&b, &a, &c);
    let mut p = Project::open(dir.path()).unwrap();
    let s = p.merge_branch("incoming").unwrap();
    let id = s.merge.unwrap().conflicts[0].id.clone();
    let s = p
        .resolve_conflict(
            id.clone(),
            Resolution::Custom("combined\r\n本文".into()),
            s.revision,
        )
        .unwrap();
    assert_eq!(s.merge.unwrap().remaining, 0);
    assert_eq!(
        Project::open(dir.path())
            .unwrap()
            .snapshot()
            .merge
            .unwrap()
            .remaining,
        1
    );
    p.complete_merge("Comment merge", s.revision).unwrap();
    let s = p.snapshot();
    let comment = s.data.masters["enemy"]
        .data
        .as_ref()
        .unwrap()
        .comments
        .table
        .as_ref()
        .unwrap();
    assert_eq!(comment.body, "combined\n本文");
    assert_eq!(comment.created_by.name, "creator");
    assert_eq!(comment.updated_by.name, "Resolver");
    assert!(comment.updated_at.ends_with('Z'));
    assert!(!s.git.tracked_dirty);
    let detail = p
        .history_detail(&git(dir.path(), &["rev-parse", "HEAD"]).unwrap())
        .unwrap();
    assert!(detail.changes.iter().any(
        |c| matches!(c, SemanticChange::Comment { after: Some(v), .. } if v == "combined\n本文")
    ));
    assert!(serde_json::from_str::<Resolution>(r#"{"kind":"comment","value":null}"#).is_err());
}
#[test]
fn blank_manual_comment_resolution_removes_last_file() {
    let mut b = data();
    comments(&mut b).table = comment("base", 0, "b");
    let mut a = b.clone();
    let mut c = b.clone();
    comments(&mut a).table = None;
    comments(&mut c).table.as_mut().unwrap().body = "changed".into();
    let dir = repo(&b, &a, &c);
    let mut p = Project::open(dir.path()).unwrap();
    let s = p.merge_branch("incoming").unwrap();
    let id = s.merge.unwrap().conflicts[0].id.clone();
    let s = p
        .resolve_conflict(id, Resolution::Custom(" \r\n ".into()), s.revision)
        .unwrap();
    p.complete_merge("delete comment", s.revision).unwrap();
    assert!(!dir
        .path()
        .join("gamemasterstudio/comments/enemy.json")
        .exists());
    assert!(!p.snapshot().git.tracked_dirty);
}
#[test]
fn independent_comment_edits_auto_complete_git_merge() {
    let b = data();
    let mut a = b.clone();
    let mut c = b.clone();
    comments(&mut a).table = comment("table", 1, "a");
    comments(&mut c).set(
        &CommentTarget::Row {
            primary_key: vec!["1".into(), "2".into()],
        },
        "row",
        &author("c"),
    );
    let dir = repo(&b, &a, &c);
    let mut p = Project::open(dir.path()).unwrap();
    let s = p.merge_branch("incoming").unwrap();
    assert!(s.merge.is_none());
    let m = &s.data.masters["enemy"].data.as_ref().unwrap().comments;
    assert!(m.table.is_some());
    assert_eq!(m.rows.len(), 1);
    assert!(!s.git.tracked_dirty);
}
#[test]
fn history_is_paginated_anchored_and_read_only() {
    let b = data();
    let dir = repo(&b, &b, &b);
    for n in 0..51 {
        save(dir.path(), &format!("履歴 {n}"));
    }
    let p = Project::open(dir.path()).unwrap();
    let first = p.history(None, 0).unwrap();
    assert_eq!(first.commits.len(), 50);
    assert!(first.has_more);
    assert_eq!(first.commits[0].subject, "履歴 50");
    save(dir.path(), "new commit");
    let next = p.history(first.head.as_deref(), 50).unwrap();
    assert_eq!(next.commits.len(), 3);
    assert!(!next.has_more);
    let initial = next.commits.last().unwrap();
    let before = fs::read(dir.path().join(".git/index")).unwrap();
    let detail = p.history_detail(&initial.oid).unwrap();
    assert!(detail.parent.is_none());
    assert!(detail
        .changes
        .iter()
        .any(|c| matches!(c, SemanticChange::AddedMaster { .. })));
    assert_eq!(fs::read(dir.path().join(".git/index")).unwrap(), before);
    assert!(p.history_detail("--all").is_err());
    assert!(p.history_detail(&"f".repeat(40)).is_err());
}
#[test]
fn history_reports_corrupt_snapshot_instead_of_fake_deletion() {
    let b = data();
    let dir = repo(&b, &b, &b);
    let p = Project::open(dir.path()).unwrap();
    fs::write(dir.path().join("masters/enemy.csv"), "bad\nvalue\n").unwrap();
    save(dir.path(), "broken");
    let oid = git(dir.path(), &["rev-parse", "HEAD"]).unwrap();
    assert!(p.history_detail(&oid).is_err());
}
#[test]
fn protected_settings_validate_autosave_undo_review_and_commit() {
    let b = data();
    let dir = repo(&b, &b, &b);
    let mut p = Project::open(dir.path()).unwrap();
    apply(
        &mut p,
        Operation::SetProtectedBranches {
            patterns: vec!["main".into(), "release/*".into(), "hotfix/?[0-9]".into()],
        },
    );
    assert!(p
        .semantic_diff()
        .unwrap()
        .iter()
        .any(|c| matches!(c, SemanticChange::ProjectConfig { .. })));
    assert_eq!(
        Project::open(dir.path())
            .unwrap()
            .snapshot()
            .data
            .config
            .git
            .protected_branches
            .len(),
        3
    );
    p.undo(p.snapshot().revision).unwrap();
    assert_eq!(
        p.snapshot().data.config.git.protected_branches,
        vec!["main"]
    );
    p.redo(p.snapshot().revision).unwrap();
    p.commit("Protection", false).unwrap();
    for branch in ["release/1", "hotfix/a1"] {
        p.switch_branch(branch, true).unwrap();
        assert!(p.snapshot().git.protected);
        assert!(p.commit("forbidden", false).is_err());
        assert!(p.merge_branch("incoming").is_err());
        assert!(p.undo(p.snapshot().revision).is_err());
        assert!(!p.history(None, 0).unwrap().commits.is_empty());
        p.fetch().unwrap();
        p.switch_branch("work", false).unwrap();
    }
    p.switch_branch("Release/2", true).unwrap();
    assert!(!p.snapshot().git.protected);
    for patterns in [
        vec!["["],
        vec![" main"],
        vec!["main", "main"],
        vec!["Release/*"],
    ] {
        assert!(p
            .apply(
                Operation::SetProtectedBranches {
                    patterns: patterns.into_iter().map(str::to_owned).collect()
                },
                p.snapshot().revision
            )
            .is_err());
    }
}
#[test]
fn unborn_protected_branch_requires_working_branch_and_identity_for_commit() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = Project::initialize(dir.path()).unwrap();
    assert!(p.history(None, 0).unwrap().commits.is_empty());
    p.set_identity("Tester", "tester@example.com").unwrap();
    assert!(p
        .apply(
            Operation::SetProtectedBranches { patterns: vec![] },
            p.snapshot().revision
        )
        .is_err());
    p.switch_branch("work", true).unwrap();
    p.commit("Project initialized", false).unwrap();
    git(dir.path(), &["config", "user.name", ""]).unwrap();
    p = Project::open(dir.path()).unwrap();
    assert!(p.commit("no identity", false).is_err());
}

#[test]
fn all_comment_scopes_conflict_on_delete_modify_and_choose_independently() {
    let mut b = data();
    let targets = [
        CommentTarget::Table,
        CommentTarget::Row {
            primary_key: vec!["1".into(), "2".into()],
        },
        CommentTarget::Cell {
            primary_key: vec!["1".into(), "2".into()],
            column: "hp".into(),
        },
    ];
    for t in &targets {
        comments(&mut b).set(t, "base", &author("base"));
    }
    let mut a = b.clone();
    let mut c = b.clone();
    for t in &targets {
        comments(&mut a).set(t, "", &author("a"));
        comments(&mut c).set(t, "changed", &author("c"));
    }
    let plan = merge_project(&b, &a, &c, &BTreeMap::new()).unwrap();
    assert_eq!(plan.view.remaining, 3);
    let choices = plan
        .view
        .conflicts
        .iter()
        .map(|c| {
            (
                c.id.clone(),
                if c.column.is_some() {
                    Resolution::Theirs
                } else {
                    Resolution::Ours
                },
            )
        })
        .collect();
    let mut result = merge_project(&b, &a, &c, &choices).unwrap();
    assert_eq!(result.view.remaining, 0);
    let comments = comments(&mut result.data);
    assert!(comments.table.is_none());
    assert!(comments.rows.is_empty());
    assert_eq!(comments.cells[0].comment.body, "changed");
}
#[test]
fn master_deletion_ignores_comment_metadata_only_changes() {
    let mut b = data();
    comments(&mut b).table = comment("same", 0, "base");
    let mut a = b.clone();
    a.config.masters.clear();
    a.masters.clear();
    let mut c = b.clone();
    comments(&mut c).table = comment("same", 2, "other");
    let plan = merge_project(&b, &a, &c, &BTreeMap::new()).unwrap();
    assert_eq!(plan.view.remaining, 0);
    assert!(plan.data.masters.is_empty());
    comments(&mut c).table.as_mut().unwrap().body = "changed".into();
    assert_eq!(
        merge_project(&b, &a, &c, &BTreeMap::new())
            .unwrap()
            .view
            .conflicts[0]
            .kind,
        "master"
    );
}
#[test]
fn protected_update_is_fast_forward_only_even_without_identity() {
    let b = data();
    let dir = repo(&b, &b, &b);
    let mut p = Project::open(dir.path()).unwrap();
    git(
        dir.path(),
        &["branch", "--set-upstream-to=incoming", "main"],
    )
    .unwrap();
    p.switch_branch("main", false).unwrap();
    git(dir.path(), &["config", "user.name", ""]).unwrap();
    p = Project::open(dir.path()).unwrap();
    p.update().unwrap();
    assert_eq!(
        git(dir.path(), &["rev-parse", "HEAD"]).unwrap(),
        git(dir.path(), &["rev-parse", "incoming"]).unwrap()
    );
    git(dir.path(), &["config", "user.name", "Tester"]).unwrap();
    save(dir.path(), "external main change");
    let before = git(dir.path(), &["rev-parse", "HEAD"]).unwrap();
    git(dir.path(), &["switch", "incoming"]).unwrap();
    save(dir.path(), "incoming divergence");
    git(dir.path(), &["switch", "main"]).unwrap();
    p = Project::open(dir.path()).unwrap();
    assert!(p.update().is_err());
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]).unwrap(), before);
    assert!(!p.snapshot().git.merge_in_progress);
}
#[test]
fn history_before_project_initialization_has_no_invented_changes() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    git(r, &["init", "-b", "work"]).unwrap();
    git(r, &["config", "user.name", "Tester"]).unwrap();
    git(r, &["config", "user.email", "test@example.com"]).unwrap();
    git(r, &["config", "commit.gpgsign", "false"]).unwrap();
    save(r, "Before Project");
    let initial = git(r, &["rev-parse", "HEAD"]).unwrap();
    let p = Project::initialize(r).unwrap();
    assert!(p.history_detail(&initial).unwrap().changes.is_empty());
}
