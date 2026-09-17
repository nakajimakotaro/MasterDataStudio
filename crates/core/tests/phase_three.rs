use gamemasterstudio_core::{
    comments::Comments,
    config::{MasterDefinition, ProjectConfig, CONFIG_PATH},
    csv_data::Table,
    merge::{merge_project, three_way, MergePlan, Resolution},
    project::{git, Master, MasterEntry, Operation, Project, ProjectData},
};
use std::{collections::BTreeMap, fs, path::Path};

fn data(csv: &str) -> ProjectData {
    let def = MasterDefinition {
        path: "masters/enemy.csv".into(),
        primary_key: vec!["id".into(), "wave".into()],
    };
    let table = Table::parse(csv.as_bytes(), &def).unwrap();
    ProjectData {
        config: ProjectConfig {
            masters: BTreeMap::from([("enemy".into(), def)]),
            ..ProjectConfig::default()
        },
        masters: BTreeMap::from([(
            "enemy".into(),
            MasterEntry {
                data: Some(Master {
                    scripts: Default::default(),
                    script_error: None,
                    table,
                    comments: Comments::default(),
                }),
                error: None,
            },
        )]),
    }
}
fn empty() -> ProjectData {
    ProjectData {
        config: ProjectConfig::default(),
        masters: BTreeMap::new(),
    }
}
fn plan(b: &ProjectData, a: &ProjectData, c: &ProjectData) -> MergePlan {
    merge_project(b, a, c, &BTreeMap::new()).unwrap()
}
fn table(p: &MergePlan) -> &Table {
    &p.data.masters["enemy"].data.as_ref().unwrap().table
}
fn choose_all(
    b: &ProjectData,
    a: &ProjectData,
    c: &ProjectData,
    resolution: Resolution,
) -> MergePlan {
    let initial = plan(b, a, c);
    let choices = initial
        .view
        .conflicts
        .iter()
        .map(|c| (c.id.clone(), resolution.clone()))
        .collect();
    merge_project(b, a, c, &choices).unwrap()
}

#[test]
fn cell_truth_table_including_absent_and_empty() {
    let values = [None, Some(""), Some("001"), Some("1")];
    for b in values {
        for a in values {
            for c in values {
                let result = three_way(&b, &a, &c);
                if a == c || b == c {
                    assert_eq!(result, Some(a));
                } else if a == b {
                    assert_eq!(result, Some(c));
                } else {
                    assert_eq!(result, None);
                }
            }
        }
    }
    assert_eq!(three_way(&None, &Some(""), &Some("foo")), None);
}
#[test]
fn composite_keys_independent_cells_and_row_order_merge() {
    let b = data("id,wave,hp,attack\n1,2,100,20\n1,10,200,30\n");
    let a = data("id,wave,hp,attack\n1,10,200,30\n1,2,120,20\n");
    let c = data("id,wave,hp,attack\n1,2,100,25\n1,10,200,30\n");
    let p = plan(&b, &a, &c);
    assert_eq!(p.view.remaining, 0);
    assert_eq!(
        table(&p).rows,
        vec![vec!["1", "10", "200", "30"], vec!["1", "2", "120", "25"]]
    );
    assert_eq!(p.view.automatically_merged, 2);
}
#[test]
fn concurrent_row_additions_only_conflict_on_different_cells() {
    let b = data("id,wave,hp,attack\n");
    let a = data("id,wave,hp,attack\n1,1,,20\n");
    let c = data("id,wave,hp,attack\n1,1,100,20\n");
    let p = plan(&b, &a, &c);
    assert_eq!(p.view.remaining, 1);
    let conflict = &p.view.conflicts[0];
    assert_eq!(conflict.kind, "cell");
    assert_eq!(conflict.column.as_deref(), Some("hp"));
    assert!(conflict.base.is_null());
    assert_eq!(conflict.ours, "");
    let p = choose_all(&b, &a, &c, Resolution::Custom("001\r\nline".into()));
    assert_eq!(table(&p).rows[0][2], "001\nline");
    assert_eq!(p.view.remaining, 0);
}
#[test]
fn column_order_and_disjoint_added_column_values_are_stable() {
    let b = data("id,wave,hp\n1,1,100\n");
    let a = data("id,wave,hp,defense,shared\n1,1,100,3,same\n");
    let c = data("id,wave,hp,speed,shared\n1,1,100,4,same\n");
    let p = plan(&b, &a, &c);
    assert_eq!(p.view.remaining, 0);
    assert_eq!(
        table(&p).columns,
        vec!["id", "wave", "hp", "defense", "shared", "speed"]
    );
    assert_eq!(table(&p).rows[0], vec!["1", "1", "100", "3", "same", "4"]);
}
#[test]
fn added_rows_with_disjoint_columns_use_absent_not_empty() {
    let b = data("id,wave\n");
    let a = data("id,wave,hp\n1,1,001\n");
    let c = data("id,wave,attack\n1,1,2\n");
    let p = plan(&b, &a, &c);
    assert_eq!(p.view.remaining, 0);
    assert_eq!(table(&p).rows[0], vec!["1", "1", "001", "2"]);
}
#[test]
fn row_delete_unchanged_and_delete_modify_are_distinct() {
    let b = data("id,wave,hp\n1,1,100\n");
    let a = data("id,wave,hp\n");
    assert!(table(&plan(&b, &a, &b)).rows.is_empty());
    assert!(table(&plan(&b, &a, &a)).rows.is_empty());
    let c = data("id,wave,hp\n1,1,120\n");
    assert_eq!(plan(&b, &a, &c).view.conflicts[0].kind, "row");
    assert!(table(&choose_all(&b, &a, &c, Resolution::Ours))
        .rows
        .is_empty());
    assert_eq!(
        table(&choose_all(&b, &a, &c, Resolution::Theirs)).rows[0][2],
        "120"
    );
}
#[test]
fn column_delete_modify_and_delete_unchanged() {
    let b = data("id,wave,hp\n1,1,100\n");
    let a = data("id,wave\n1,1\n");
    assert_eq!(table(&plan(&b, &a, &b)).columns, vec!["id", "wave"]);
    let c = data("id,wave,hp\n1,1,120\n");
    let p = plan(&b, &a, &c);
    assert_eq!(p.view.remaining, 1);
    assert_eq!(p.view.conflicts[0].kind, "column");
    let p = choose_all(&b, &a, &c, Resolution::Theirs);
    assert_eq!(p.view.remaining, 0);
    assert_eq!(table(&p).rows[0][2], "120");
}
#[test]
fn master_add_delete_and_modify_conflicts() {
    let b = data("id,wave,hp\n1,1,100\n");
    let c = data("id,wave,hp\n1,1,120\n");
    assert!(plan(&b, &empty(), &b).data.masters.is_empty());
    assert_eq!(
        table(&plan(&empty(), &b, &empty())),
        &b.masters["enemy"].data.as_ref().unwrap().table
    );
    assert_eq!(plan(&b, &empty(), &c).view.conflicts[0].kind, "master");
    assert_eq!(
        table(&choose_all(&b, &empty(), &c, Resolution::Theirs)).rows[0][2],
        "120"
    );
    let p = plan(&empty(), &b, &c);
    assert_eq!(p.view.remaining, 1);
    assert_eq!(p.view.conflicts[0].kind, "cell");
}
#[test]
fn changed_path_or_key_requires_whole_master_definition_choice() {
    let b = data("id,wave,hp\n1,1,100\n");
    for added in [true, false] {
        for key_change in [true, false] {
            let mut a = b.clone();
            let def = a.config.masters.get_mut("enemy").unwrap();
            if key_change {
                def.primary_key = vec!["hp".into()];
            } else {
                def.path = "other.csv".into();
            }
            let base = if added { empty() } else { b.clone() };
            let p = plan(&base, &a, &b);
            assert_eq!(p.view.remaining, 1);
            assert_eq!(p.view.conflicts[0].kind, "projectConfig");
            let p = choose_all(&base, &a, &b, Resolution::Theirs);
            assert_eq!(p.data.config.masters["enemy"], b.config.masters["enemy"]);
            assert_eq!(p.view.remaining, 0);
        }
    }
}

fn write_project(root: &Path, p: &ProjectData) {
    fs::create_dir_all(root.join("gamemasterstudio")).unwrap();
    fs::write(root.join(CONFIG_PATH), p.config.serialize().unwrap()).unwrap();
    for (id, def) in &p.config.masters {
        let path = root.join(&def.path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            p.masters[id]
                .data
                .as_ref()
                .unwrap()
                .table
                .serialize(def)
                .unwrap(),
        )
        .unwrap();
    }
}
fn commit_all(root: &Path) {
    git(root, &["add", "-A"]).unwrap();
    git(root, &["commit", "-m", "fixture"]).unwrap();
}
fn repository(base: &ProjectData, ours: &ProjectData, theirs: &ProjectData) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-b", "work"]).unwrap();
    git(root, &["config", "user.name", "Merge Tester"]).unwrap();
    git(root, &["config", "user.email", "merge@example.com"]).unwrap();
    git(root, &["config", "commit.gpgsign", "false"]).unwrap();
    write_project(root, base);
    commit_all(root);
    git(root, &["switch", "-c", "incoming"]).unwrap();
    write_project(root, theirs);
    commit_all(root);
    git(root, &["switch", "work"]).unwrap();
    write_project(root, ours);
    commit_all(root);
    dir
}
fn conflict_repo() -> tempfile::TempDir {
    repository(
        &data("id,wave,hp\n1,1,100\n"),
        &data("id,wave,hp\n1,1,120\n"),
        &data("id,wave,hp\n1,1,150\n"),
    )
}
#[test]
fn git_stages_reopen_custom_resolution_stage_and_merge_commit() {
    let dir = conflict_repo();
    let root = dir.path();
    let mut project = Project::open(root).unwrap();
    let s = project.merge_branch("incoming").unwrap();
    assert_eq!(s.merge.unwrap().remaining, 1);
    let original_index = git(root, &["ls-files", "-u"]).unwrap();
    assert!(!original_index.is_empty());
    // Even invalid working-tree content must never be a merge input.
    fs::write(
        root.join("masters/enemy.csv"),
        "not a csv\n<<<<<<< corrupt\n",
    )
    .unwrap();
    let mut reopened = Project::open(root).unwrap();
    let s = reopened.snapshot();
    let view = s.merge.unwrap();
    assert_eq!(view.conflicts[0].base, "100");
    assert_eq!(view.conflicts[0].ours, "120");
    assert_eq!(view.conflicts[0].theirs, "150");
    assert!(reopened.complete_merge("", s.revision).is_err());
    assert!(reopened.commit("bypass", false).is_err());
    assert!(reopened
        .apply(
            Operation::AddColumn {
                master_id: "enemy".into(),
                name: "invalid".into()
            },
            s.revision
        )
        .is_err());
    assert!(reopened.switch_branch("other", true).is_err());
    let s = reopened
        .resolve_conflict(
            view.conflicts[0].id.clone(),
            Resolution::Custom("".into()),
            s.revision,
        )
        .unwrap();
    assert_eq!(git(root, &["ls-files", "-u"]).unwrap(), original_index);
    assert_eq!(
        Project::open(root)
            .unwrap()
            .snapshot()
            .merge
            .unwrap()
            .remaining,
        1
    );
    let s = reopened.complete_merge("resolved", s.revision).unwrap();
    assert!(s.merge.is_none());
    assert!(!s.git.merge_in_progress);
    assert!(!s.git.tracked_dirty);
    assert_eq!(
        fs::read_to_string(root.join("masters/enemy.csv")).unwrap(),
        "id,wave,hp\n1,1,\n"
    );
    assert_eq!(
        git(root, &["rev-list", "--parents", "-n", "1", "HEAD"])
            .unwrap()
            .split_whitespace()
            .count(),
        3
    );
}
#[test]
fn git_auto_merges_different_cells_on_same_text_line() {
    let dir = repository(
        &data("id,wave,hp,attack\n1,1,100,20\n"),
        &data("id,wave,hp,attack\n1,1,120,20\n"),
        &data("id,wave,hp,attack\n1,1,100,25\n"),
    );
    let mut project = Project::open(dir.path()).unwrap();
    let s = project.merge_branch("incoming").unwrap();
    assert!(s.merge.is_none());
    assert!(!s.git.tracked_dirty);
    assert_eq!(
        fs::read_to_string(dir.path().join("masters/enemy.csv")).unwrap(),
        "id,wave,hp,attack\n1,1,120,25\n"
    );
}
#[test]
fn unmanaged_conflict_aborts_and_preserves_original_head() {
    let dir = conflict_repo();
    let root = dir.path();
    // Add the same unmanaged path independently on both branches.
    fs::write(root.join("README.txt"), "ours").unwrap();
    commit_all(root);
    let head = git(root, &["rev-parse", "HEAD"]).unwrap();
    git(root, &["switch", "incoming"]).unwrap();
    fs::write(root.join("README.txt"), "theirs").unwrap();
    commit_all(root);
    git(root, &["switch", "work"]).unwrap();
    let mut project = Project::open(root).unwrap();
    let error = match project.merge_branch("incoming") {
        Ok(_) => panic!("must abort"),
        Err(e) => e,
    };
    assert!(error.contains("README.txt"));
    assert_eq!(git(root, &["rev-parse", "HEAD"]).unwrap(), head);
    assert!(!project.snapshot().git.merge_in_progress);
    assert_eq!(fs::read_to_string(root.join("README.txt")).unwrap(), "ours");
    assert!(git(root, &["status", "--porcelain"]).unwrap().is_empty());
}
#[test]
fn abort_and_stale_resolution_are_safe() {
    let dir = conflict_repo();
    let root = dir.path();
    let mut project = Project::open(root).unwrap();
    let s = project.merge_branch("incoming").unwrap();
    let c = &s.merge.unwrap().conflicts[0];
    assert!(project
        .resolve_conflict(c.id.clone(), Resolution::Ours, s.revision + 1)
        .is_err());
    let s = project.abort_merge().unwrap();
    assert!(s.merge.is_none());
    assert_eq!(
        fs::read_to_string(root.join("masters/enemy.csv")).unwrap(),
        "id,wave,hp\n1,1,120\n"
    );
}
#[test]
fn project_config_conflict_can_be_reopened_and_chosen_as_a_master() {
    let b = data("id,wave,hp\n1,1,100\n");
    let mut a = data("id,wave,hp\n1,1,120\n");
    a.config.masters.get_mut("enemy").unwrap().primary_key = vec!["hp".into()];
    let mut c = data("id,wave,hp\n1,1,150\n");
    c.config.masters.get_mut("enemy").unwrap().primary_key = vec!["wave".into()];
    let dir = repository(&b, &a, &c);
    let root = dir.path();
    let mut project = Project::open(root).unwrap();
    project.merge_branch("incoming").unwrap();
    assert!(fs::read_to_string(root.join(CONFIG_PATH))
        .unwrap()
        .contains("<<<<<<<"));
    let mut project = Project::open(root).unwrap();
    let s = project.snapshot();
    let v = s.merge.unwrap();
    assert_eq!(v.conflicts.len(), 1);
    assert_eq!(v.conflicts[0].kind, "projectConfig");
    let s = project
        .resolve_conflict(v.conflicts[0].id.clone(), Resolution::Theirs, s.revision)
        .unwrap();
    let s = project.complete_merge("", s.revision).unwrap();
    assert_eq!(s.data.config.masters["enemy"].primary_key, vec!["wave"]);
    assert_eq!(
        s.data.masters["enemy"].data.as_ref().unwrap().table.rows[0][2],
        "150"
    );
}
#[cfg(unix)]
#[test]
fn failed_commit_keeps_index_stages_and_can_retry() {
    use std::os::unix::fs::PermissionsExt;
    let dir = conflict_repo();
    let root = dir.path();
    let mut project = Project::open(root).unwrap();
    let s = project.merge_branch("incoming").unwrap();
    let before = git(root, &["ls-files", "-u"]).unwrap();
    let s = project
        .resolve_conflict(
            s.merge.unwrap().conflicts[0].id.clone(),
            Resolution::Theirs,
            s.revision,
        )
        .unwrap();
    let hook = root.join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(project.complete_merge("", s.revision).is_err());
    assert_eq!(git(root, &["ls-files", "-u"]).unwrap(), before);
    assert_eq!(
        Project::open(root)
            .unwrap()
            .snapshot()
            .merge
            .unwrap()
            .remaining,
        1
    );
    fs::remove_file(hook).unwrap();
    assert!(project
        .complete_merge("", s.revision)
        .unwrap()
        .merge
        .is_none());
}

#[test]
fn deleting_different_structures_does_not_create_spurious_conflicts() {
    let b = data("id,wave,hp\n1,1,100\n2,1,200\n");
    let a = data("id,wave,hp\n2,1,200\n");
    let c = data("id,wave\n1,1\n2,1\n");
    let p = plan(&b, &a, &c);
    assert_eq!(p.view.remaining, 0);
    assert_eq!(table(&p).rows, vec![vec!["2", "1"]]);
}
#[test]
fn separate_master_ids_cannot_silently_claim_the_same_csv() {
    let b = empty();
    let a = data("id,wave,hp\n1,1,100\n");
    let mut c = data("id,wave,hp\n1,1,120\n");
    let def = c.config.masters.remove("enemy").unwrap();
    c.config.masters.insert("item".into(), def);
    let master = c.masters.remove("enemy").unwrap();
    c.masters.insert("item".into(), master);
    let p = plan(&b, &a, &c);
    assert_eq!(p.view.remaining, 1);
    assert_eq!(p.view.conflicts[0].kind, "projectConfig");
    let mut p = choose_all(&b, &a, &c, Resolution::Theirs);
    p.data.config.validate().unwrap();
    assert!(p.data.masters.contains_key("item"));
    assert!(!p.data.masters.contains_key("enemy"));
}
#[test]
fn textually_clean_merge_still_checks_definition_changes() {
    let b = data("id,wave,hp\n1,1,100\n");
    let mut a = b.clone();
    a.config.masters.get_mut("enemy").unwrap().primary_key = vec!["hp".into()];
    let c = data("id,wave,hp\n1,1,150\n");
    let dir = repository(&b, &a, &c);
    let mut project = Project::open(dir.path()).unwrap();
    let s = project.merge_branch("incoming").unwrap();
    assert!(git(dir.path(), &["ls-files", "-u"]).unwrap().is_empty());
    let view = s.merge.unwrap();
    assert_eq!(view.remaining, 1);
    assert_eq!(view.conflicts[0].kind, "projectConfig");
    let mut project = Project::open(dir.path()).unwrap();
    let s = project.snapshot();
    let s = project
        .resolve_conflict(view.conflicts[0].id.clone(), Resolution::Theirs, s.revision)
        .unwrap();
    let s = project.complete_merge("", s.revision).unwrap();
    assert_eq!(
        s.data.config.masters["enemy"].primary_key,
        vec!["id", "wave"]
    );
    assert_eq!(
        s.data.masters["enemy"].data.as_ref().unwrap().table.rows[0][2],
        "150"
    );
}
#[test]
fn update_fetches_then_enters_the_same_resolver() {
    let dir = conflict_repo();
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "--bare"]).unwrap();
    git(
        dir.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    )
    .unwrap();
    git(dir.path(), &["push", "origin", "incoming"]).unwrap();
    git(
        dir.path(),
        &["branch", "--set-upstream-to=origin/incoming", "work"],
    )
    .unwrap();
    let mut project = Project::open(dir.path()).unwrap();
    let s = project.update().unwrap();
    assert_eq!(s.merge.unwrap().remaining, 1);
    project.abort_merge().unwrap();
    assert!(!project.snapshot().git.merge_in_progress);
}
#[test]
fn git_add_add_master_uses_absent_base_stage() {
    let dir = repository(
        &empty(),
        &data("id,wave,hp\n1,1,\n"),
        &data("id,wave,hp\n1,1,150\n"),
    );
    let mut project = Project::open(dir.path()).unwrap();
    let s = project.merge_branch("incoming").unwrap();
    let view = s.merge.unwrap();
    assert_eq!(view.remaining, 1);
    assert_eq!(view.conflicts[0].kind, "cell");
    assert!(view.conflicts[0].base.is_null());
    assert_eq!(view.conflicts[0].ours, "");
    let s = project
        .resolve_conflict(view.conflicts[0].id.clone(), Resolution::Ours, s.revision)
        .unwrap();
    assert!(project
        .complete_merge("", s.revision)
        .unwrap()
        .merge
        .is_none());
}
#[test]
fn delete_modify_master_conflict_can_keep_modified_master() {
    let b = data("id,wave,hp\n1,1,100\n");
    let dir = repository(&b, &empty(), &data("id,wave,hp\n1,1,150\n"));
    let root = dir.path();
    fs::remove_file(root.join("masters/enemy.csv")).unwrap();
    commit_all(root);
    let mut project = Project::open(root).unwrap();
    let s = project.merge_branch("incoming").unwrap();
    let view = s.merge.unwrap();
    assert_eq!(view.remaining, 1);
    assert_eq!(view.conflicts[0].kind, "master");
    assert!(view.conflicts[0].ours.is_null());
    let s = project
        .resolve_conflict(view.conflicts[0].id.clone(), Resolution::Theirs, s.revision)
        .unwrap();
    let s = project.complete_merge("", s.revision).unwrap();
    assert_eq!(
        s.data.masters["enemy"].data.as_ref().unwrap().table.rows[0][2],
        "150"
    );
}
#[test]
fn nonconflicting_unmanaged_files_remain_part_of_the_merge() {
    let dir = conflict_repo();
    let root = dir.path();
    fs::write(root.join("ours.txt"), "ours").unwrap();
    commit_all(root);
    git(root, &["switch", "incoming"]).unwrap();
    fs::write(root.join("theirs.txt"), "theirs").unwrap();
    commit_all(root);
    git(root, &["switch", "work"]).unwrap();
    let mut project = Project::open(root).unwrap();
    let s = project.merge_branch("incoming").unwrap();
    let c = &s.merge.unwrap().conflicts[0];
    let s = project
        .resolve_conflict(c.id.clone(), Resolution::Ours, s.revision)
        .unwrap();
    project.complete_merge("", s.revision).unwrap();
    assert_eq!(git(root, &["show", "HEAD:ours.txt"]).unwrap(), "ours");
    assert_eq!(git(root, &["show", "HEAD:theirs.txt"]).unwrap(), "theirs");
}
#[test]
fn comment_conflict_is_explicit_and_structural_deletions_prune_comments() {
    use gamemasterstudio_core::comments::{CommentTarget, Identity};
    let b = data("id,wave,hp\n1,1,100\n");
    let mut a = b.clone();
    let mut c = b.clone();
    let who = Identity {
        name: "Tester".into(),
        email: "test@example.com".into(),
    };
    for (project, body) in [(&mut a, "ours"), (&mut c, "theirs")] {
        project
            .masters
            .get_mut("enemy")
            .unwrap()
            .data
            .as_mut()
            .unwrap()
            .comments
            .set(&CommentTarget::Table, body, &who);
    }
    assert_eq!(plan(&b, &a, &c).view.conflicts[0].kind, "comment");
    let p = choose_all(&b, &a, &c, Resolution::Theirs);
    assert_eq!(
        p.data.masters["enemy"]
            .data
            .as_ref()
            .unwrap()
            .comments
            .table
            .as_ref()
            .unwrap()
            .body,
        "theirs"
    );
    let mut b = b;
    b.masters
        .get_mut("enemy")
        .unwrap()
        .data
        .as_mut()
        .unwrap()
        .comments
        .set(
            &CommentTarget::Cell {
                primary_key: vec!["1".into(), "1".into()],
                column: "hp".into(),
            },
            "note",
            &who,
        );
    let a = data("id,wave\n1,1\n");
    let p = plan(&b, &a, &b);
    assert_eq!(p.view.remaining, 0);
    assert!(p.data.masters["enemy"]
        .data
        .as_ref()
        .unwrap()
        .comments
        .cells
        .is_empty());
}

#[test]
fn removing_a_conflicting_column_eliminates_its_row_conflict() {
    let b = data("id,wave,hp\n1,1,100\n");
    let a = data("id,wave\n");
    let c = data("id,wave,hp\n1,1,150\n");
    let p = plan(&b, &a, &c);
    assert_eq!(p.view.conflicts.len(), 1);
    assert_eq!(p.view.conflicts[0].kind, "column");
    let p = choose_all(&b, &a, &c, Resolution::Ours);
    assert_eq!(p.view.remaining, 0);
    assert!(table(&p).rows.is_empty());
    let p = choose_all(&b, &a, &c, Resolution::Theirs);
    assert_eq!(p.view.remaining, 1);
    assert!(p
        .view
        .conflicts
        .iter()
        .any(|c| c.kind == "row" && c.resolution.is_none()));
}

#[test]
fn twenty_thousand_conflicts_resolve_atomically_in_batches() {
    let csv = |value: &str| {
        let mut csv = String::from("id,wave,hp\n");
        for i in 0..20_000 {
            csv.push_str(&format!("{i},1,{value}\n"));
        }
        csv
    };
    let dir = repository(&data(&csv("100")), &data(&csv("120")), &data(&csv("150")));
    let mut project = Project::open(dir.path()).unwrap();
    let s = project.merge_branch("incoming").unwrap();
    let view = s.merge.unwrap();
    assert_eq!(view.remaining, 20_000);
    let ids: Vec<_> = view.conflicts.iter().map(|c| c.id.clone()).collect();
    // A bad ID at the end must leave even the first valid choice untouched.
    let mut invalid = ids.clone();
    invalid.push("missing".into());
    assert!(project
        .resolve_conflicts(invalid, Resolution::Theirs, s.revision)
        .is_err());
    assert_eq!(project.snapshot().merge.unwrap().remaining, 20_000);
    assert!(project
        .resolve_conflicts(ids.clone(), Resolution::Ours, s.revision + 1)
        .is_err());
    assert!(project
        .resolve_conflicts(vec![], Resolution::Ours, s.revision)
        .is_err());
    assert!(project
        .resolve_conflicts(ids.clone(), Resolution::Custom("bad".into()), s.revision)
        .is_err());
    let s = project
        .resolve_conflicts(ids[..10_000].to_vec(), Resolution::Theirs, s.revision)
        .unwrap();
    assert_eq!(s.merge.as_ref().unwrap().remaining, 10_000);
    assert_eq!(s.revision, 2);
    let s = project
        .resolve_conflicts(ids[10_000..].to_vec(), Resolution::Ours, s.revision)
        .unwrap();
    assert_eq!(s.merge.as_ref().unwrap().remaining, 0);
    let s = project.complete_merge("bulk resolved", s.revision).unwrap();
    let rows = &s.data.masters["enemy"].data.as_ref().unwrap().table.rows;
    assert_eq!(rows.len(), 20_000);
    assert_eq!(rows.iter().filter(|r| r[2] == "150").count(), 10_000);
    assert_eq!(rows.iter().filter(|r| r[2] == "120").count(), 10_000);
}
