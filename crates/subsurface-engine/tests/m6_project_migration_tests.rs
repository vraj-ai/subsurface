use rusqlite::Connection;
use serde_json::json;
use std::sync::Arc;
use subsurface_engine::fixture::GitFixture;
use subsurface_engine::mcp::{McpServer, McpToolCall};
use subsurface_engine::provider::FakeProvider;
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

#[test]
fn mcp_accepts_site_emits_project() {
    let store = Arc::new(SqliteStore::in_memory().expect("store"));
    let mcp = McpServer::new(store, Arc::new(FakeProvider::new("Local rationale")));
    let mut fixture = GitFixture::new();
    fixture.commit("initial", &[("src/lib.rs", "fn example() {}\n")]);

    let legacy_call: McpToolCall = serde_json::from_value(json!({
        "Excavate": {
            "site_path": fixture.path().to_string_lossy(),
            "file_path": "src/lib.rs",
            "start_line": 1,
            "end_line": 1
        }
    }))
    .expect("legacy site_path input");

    let result = mcp.handle_tool_call(legacy_call).expect("MCP excavate");
    let canonical_path = result.finding.site_path.to_string_lossy().to_string();
    let response = serde_json::to_value(result).expect("serialize response");

    assert_eq!(response["finding"]["project_path"], canonical_path);
    assert!(response["finding"].get("site_path").is_none());
}
