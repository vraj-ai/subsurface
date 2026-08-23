use subsurface_engine::store::{ActivityStatus, SqliteStore};

#[test]
fn activity_survives_store_reopen() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db_path = temp.path().join("subsurface.db");
    let project_path = temp.path().join("project");

    let store = SqliteStore::open(&db_path).expect("store");
    let id = store
        .record_activity(&project_path, "assessment", "Assess Project")
        .expect("record activity");
    assert!(store
        .update_activity(&id, ActivityStatus::Succeeded, Some("Grade improved"))
        .expect("update activity"));
    store
        .update_activity(&id, ActivityStatus::Succeeded, None)
        .expect("status-only update");
    store
        .record_activity(&temp.path().join("other-project"), "assessment", "Other Project")
        .expect("record isolated activity");
    drop(store);

    let reopened = SqliteStore::open(&db_path).expect("reopen store");
    let activities = reopened.list_activities(&project_path).expect("list activities");

    assert_eq!(activities.len(), 1);
    assert_eq!(activities[0].id, id);
    assert_eq!(activities[0].status, ActivityStatus::Succeeded);
    assert_eq!(activities[0].detail.as_deref(), Some("Grade improved"));
}
