use rusqlite::Connection;
use subsurface_engine::store::SqliteStore;

#[test]
fn migrates_legacy_site_rows_without_loss() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db_path = temp.path().join("subsurface.db");
    let project_path = "/tmp/example-project";

    let legacy = Connection::open(&db_path).expect("legacy database");
    legacy
        .execute_batch(
            "CREATE TABLE workspace_sites (
                site_path TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                is_pinned INTEGER NOT NULL DEFAULT 0,
                last_opened TEXT NOT NULL
            );
            CREATE TABLE field_notes (
                id TEXT PRIMARY KEY,
                site_path TEXT NOT NULL,
                file_path TEXT NOT NULL,
                line_start INTEGER NOT NULL,
                line_end INTEGER NOT NULL,
                commit_sha TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                user_notes TEXT NOT NULL,
                finding_json TEXT NOT NULL
            );
            CREATE TABLE finding_cache (
                site_path TEXT NOT NULL,
                file_path TEXT NOT NULL,
                line_start INTEGER NOT NULL,
                line_end INTEGER NOT NULL,
                head_commit_sha TEXT NOT NULL,
                created_at TEXT NOT NULL,
                finding_json TEXT NOT NULL,
                PRIMARY KEY (site_path, file_path, line_start, line_end)
            );",
        )
        .expect("legacy schema");
    legacy
        .execute(
            "INSERT INTO workspace_sites VALUES (?1, 'Example', 1, '2026-01-01')",
            [project_path],
        )
        .expect("legacy workspace row");
    legacy
        .execute(
            "INSERT INTO field_notes VALUES ('note', ?1, 'src/lib.rs', 1, 1, 'abc', 'now', 'now', 'keep', '{}')",
            [project_path],
        )
        .expect("legacy note row");
    legacy
        .execute(
            "INSERT INTO finding_cache VALUES (?1, 'src/lib.rs', 1, 1, 'abc', 'now', '{}')",
            [project_path],
        )
        .expect("legacy cache row");
    drop(legacy);

    let store = SqliteStore::open(&db_path).expect("migrate legacy database");
    let projects = store
        .list_workspace_sites()
        .expect("list migrated projects");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].site_path.to_string_lossy(), project_path);
    assert!(projects[0].is_pinned);
    drop(store);

    for _ in 0..2 {
        let store = SqliteStore::open(&db_path).expect("idempotent reopen");
        drop(store);
    }

    let migrated = Connection::open(&db_path).expect("migrated database");
    for table in ["workspace_sites", "field_notes", "finding_cache"] {
        let (site, project, count): (String, String, i64) = migrated
            .query_row(
                &format!(
                    "SELECT site_path, project_path, COUNT(*) FROM {table} GROUP BY site_path, project_path"
                ),
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("migrated row");
        assert_eq!(site, project_path);
        assert_eq!(project, project_path);
        assert_eq!(count, 1);
    }
}
