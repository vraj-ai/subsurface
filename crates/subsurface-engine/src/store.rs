use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use thiserror::Error;
use uuid::Uuid;

use crate::evidence::LineRange;
use crate::excavate::Finding;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Lock error")]
    Lock,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldNote {
    pub id: String,
    pub site_path: PathBuf,
    pub file_path: String,
    pub line_range: LineRange,
    pub commit_sha: String,
    pub created_at: String,
    pub updated_at: String,
    pub user_notes: String,
    pub finding: Finding,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceSiteRecord {
    pub site_path: PathBuf,
    pub name: String,
    pub is_pinned: bool,
    pub last_opened: String,
}

pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let db_path = db_path.as_ref();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_tables()?;
        Ok(store)
    }

    pub fn in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_tables()?;
        Ok(store)
    }

    fn init_tables(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS field_notes (
                id TEXT PRIMARY KEY,
                site_path TEXT NOT NULL,
                project_path TEXT,
                file_path TEXT NOT NULL,
                line_start INTEGER NOT NULL,
                line_end INTEGER NOT NULL,
                commit_sha TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                user_notes TEXT NOT NULL,
                finding_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_field_notes_site ON field_notes(site_path);
            
            CREATE TABLE IF NOT EXISTS finding_cache (
                site_path TEXT NOT NULL,
                project_path TEXT,
                file_path TEXT NOT NULL,
                line_start INTEGER NOT NULL,
                line_end INTEGER NOT NULL,
                head_commit_sha TEXT NOT NULL,
                created_at TEXT NOT NULL,
                finding_json TEXT NOT NULL,
                PRIMARY KEY (site_path, file_path, line_start, line_end)
            );
            
            CREATE TABLE IF NOT EXISTS workspace_sites (
                site_path TEXT PRIMARY KEY,
                project_path TEXT,
                name TEXT NOT NULL,
                is_pinned INTEGER NOT NULL DEFAULT 0,
                last_opened TEXT NOT NULL
            );
            
            CREATE TABLE IF NOT EXISTS scan_roots (
                path TEXT PRIMARY KEY
            );",
        )?;
        migrate_project_paths(&conn)?;
        Ok(())
    }

    pub fn record_site_opened(&self, site_path: &Path, name: &str) -> Result<(), StoreError> {
        let path_str = site_path.to_string_lossy().to_string();
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        conn.execute(
            "INSERT INTO workspace_sites (site_path, project_path, name, is_pinned, last_opened)
             VALUES (?1, ?1, ?2, 0, ?3)
             ON CONFLICT(site_path) DO UPDATE SET project_path = ?1, last_opened = ?3, name = ?2",
            params![path_str, name, now],
        )?;
        Ok(())
    }

    pub fn toggle_pin_site(&self, site_path: &Path) -> Result<bool, StoreError> {
        let path_str = site_path.to_string_lossy().to_string();
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;

        let is_pinned: bool;
        let mut stmt =
            conn.prepare("SELECT is_pinned FROM workspace_sites WHERE project_path = ?1")?;
        let mut rows = stmt.query(params![path_str])?;
        if let Some(row) = rows.next()? {
            let current: i32 = row.get(0)?;
            is_pinned = current == 0;
            conn.execute(
                "UPDATE workspace_sites SET is_pinned = ?1 WHERE project_path = ?2",
                params![if is_pinned { 1 } else { 0 }, path_str],
            )?;
        } else {
            let name = site_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "repo".to_string());
            let now = Utc::now().to_rfc3339();
            is_pinned = true;
            conn.execute(
                "INSERT INTO workspace_sites (site_path, project_path, name, is_pinned, last_opened) VALUES (?1, ?1, ?2, 1, ?3)",
                params![path_str, name, now],
            )?;
        }
        Ok(is_pinned)
    }

    pub fn list_workspace_sites(&self) -> Result<Vec<WorkspaceSiteRecord>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let mut stmt = conn.prepare(
            "SELECT project_path, name, is_pinned, last_opened FROM workspace_sites ORDER BY is_pinned DESC, last_opened DESC"
        )?;

        let rows = stmt.query_map([], |row| {
            let p: String = row.get(0)?;
            let name: String = row.get(1)?;
            let pinned_int: i32 = row.get(2)?;
            let last_opened: String = row.get(3)?;
            Ok(WorkspaceSiteRecord {
                site_path: PathBuf::from(p),
                name,
                is_pinned: pinned_int != 0,
                last_opened,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn add_scan_root(&self, path: &Path) -> Result<(), StoreError> {
        let path_str = path.to_string_lossy().to_string();
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        conn.execute(
            "INSERT OR IGNORE INTO scan_roots (path) VALUES (?1)",
            params![path_str],
        )?;
        Ok(())
    }

    pub fn list_scan_roots(&self) -> Result<Vec<PathBuf>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let mut stmt = conn.prepare("SELECT path FROM scan_roots")?;
        let rows = stmt.query_map([], |row| {
            let p: String = row.get(0)?;
            Ok(PathBuf::from(p))
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn save_field_note(
        &self,
        site_path: &Path,
        finding: &Finding,
        user_notes: &str,
    ) -> Result<String, StoreError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let site_str = site_path.to_string_lossy().to_string();
        let head_commit = finding
            .what_when
            .timeline
            .first()
            .map(|t| t.commit_sha.as_str())
            .unwrap_or("");
        let finding_json = serde_json::to_string(finding)?;

        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        conn.execute(
            "INSERT INTO field_notes (id, site_path, project_path, file_path, line_start, line_end, commit_sha, created_at, updated_at, user_notes, finding_json)
             VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                site_str,
                finding.file_path,
                finding.line_range.start as i64,
                finding.line_range.end as i64,
                head_commit,
                now,
                now,
                user_notes,
                finding_json,
            ],
        )?;

        Ok(id)
    }

    pub fn get_field_note(&self, id: &str) -> Result<Option<FieldNote>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let mut stmt = conn.prepare(
            "SELECT id, project_path, file_path, line_start, line_end, commit_sha, created_at, updated_at, user_notes, finding_json
             FROM field_notes WHERE id = ?1",
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let site_path_str: String = row.get(1)?;
            let file_path: String = row.get(2)?;
            let line_start: usize = row.get(3)?;
            let line_end: usize = row.get(4)?;
            let commit_sha: String = row.get(5)?;
            let created_at: String = row.get(6)?;
            let updated_at: String = row.get(7)?;
            let user_notes: String = row.get(8)?;
            let finding_json: String = row.get(9)?;
            let finding: Finding = serde_json::from_str(&finding_json)?;

            Ok(Some(FieldNote {
                id,
                site_path: PathBuf::from(site_path_str),
                file_path,
                line_range: LineRange {
                    start: line_start,
                    end: line_end,
                },
                commit_sha,
                created_at,
                updated_at,
                user_notes,
                finding,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_field_notes(&self, site_path: &Path) -> Result<Vec<FieldNote>, StoreError> {
        let site_str = site_path.to_string_lossy().to_string();
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let mut stmt = conn.prepare(
            "SELECT id, project_path, file_path, line_start, line_end, commit_sha, created_at, updated_at, user_notes, finding_json
             FROM field_notes WHERE project_path = ?1 ORDER BY created_at DESC",
        )?;

        let rows = stmt.query_map(params![site_str], |row| {
            let id: String = row.get(0)?;
            let site_path_str: String = row.get(1)?;
            let file_path: String = row.get(2)?;
            let line_start: usize = row.get(3)?;
            let line_end: usize = row.get(4)?;
            let commit_sha: String = row.get(5)?;
            let created_at: String = row.get(6)?;
            let updated_at: String = row.get(7)?;
            let user_notes: String = row.get(8)?;
            let finding_json: String = row.get(9)?;
            Ok((
                id,
                site_path_str,
                file_path,
                line_start,
                line_end,
                commit_sha,
                created_at,
                updated_at,
                user_notes,
                finding_json,
            ))
        })?;

        let mut notes = Vec::new();
        for r in rows {
            let (
                id,
                site_path_str,
                file_path,
                line_start,
                line_end,
                commit_sha,
                created_at,
                updated_at,
                user_notes,
                finding_json,
            ) = r?;
            let finding: Finding = serde_json::from_str(&finding_json)?;
            notes.push(FieldNote {
                id,
                site_path: PathBuf::from(site_path_str),
                file_path,
                line_range: LineRange {
                    start: line_start,
                    end: line_end,
                },
                commit_sha,
                created_at,
                updated_at,
                user_notes,
                finding,
            });
        }
        Ok(notes)
    }

    pub fn search_field_notes(
        &self,
        site_path: &Path,
        query: &str,
    ) -> Result<Vec<FieldNote>, StoreError> {
        let all = self.list_field_notes(site_path)?;
        let q = query.to_lowercase();
        Ok(all
            .into_iter()
            .filter(|n| {
                n.user_notes.to_lowercase().contains(&q)
                    || n.file_path.to_lowercase().contains(&q)
                    || n.finding.why.rationale.to_lowercase().contains(&q)
            })
            .collect())
    }

    pub fn count_field_notes(&self, site_path: &Path) -> Result<usize, StoreError> {
        let site_str = site_path.to_string_lossy().to_string();
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM field_notes WHERE project_path = ?1")?;
        let count: i64 = stmt.query_row(params![site_str], |r| r.get(0))?;
        Ok(count as usize)
    }

    pub fn delete_field_note(&self, id: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let count = conn.execute("DELETE FROM field_notes WHERE id = ?1", params![id])?;
        Ok(count > 0)
    }

    pub fn cache_finding(
        &self,
        site_path: &Path,
        file_path: &str,
        line_range: LineRange,
        head_commit_sha: &str,
        finding: &Finding,
    ) -> Result<(), StoreError> {
        let site_str = site_path.to_string_lossy().to_string();
        let now = Utc::now().to_rfc3339();
        let finding_json = serde_json::to_string(finding)?;

        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        conn.execute(
            "INSERT OR REPLACE INTO finding_cache (site_path, project_path, file_path, line_start, line_end, head_commit_sha, created_at, finding_json)
             VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                site_str,
                file_path,
                line_range.start as i64,
                line_range.end as i64,
                head_commit_sha,
                now,
                finding_json,
            ],
        )?;

        Ok(())
    }

    pub fn get_cached_finding(
        &self,
        site_path: &Path,
        file_path: &str,
        line_range: LineRange,
        head_commit_sha: &str,
    ) -> Result<Option<Finding>, StoreError> {
        let site_str = site_path.to_string_lossy().to_string();
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let mut stmt = conn.prepare(
            "SELECT head_commit_sha, finding_json FROM finding_cache
             WHERE project_path = ?1 AND file_path = ?2 AND line_start = ?3 AND line_end = ?4",
        )?;

        let mut rows = stmt.query(params![
            site_str,
            file_path,
            line_range.start as i64,
            line_range.end as i64
        ])?;

        if let Some(row) = rows.next()? {
            let cached_sha: String = row.get(0)?;
            let finding_json: String = row.get(1)?;

            if cached_sha == head_commit_sha
                || self.is_cache_valid_for_head(
                    site_path,
                    file_path,
                    &cached_sha,
                    head_commit_sha,
                )?
            {
                let finding: Finding = serde_json::from_str(&finding_json)?;
                return Ok(Some(finding));
            }
        }

        Ok(None)
    }

    pub fn is_cache_valid_for_head(
        &self,
        site_path: &Path,
        file_path: &str,
        cached_head_sha: &str,
        current_head_sha: &str,
    ) -> Result<bool, StoreError> {
        if cached_head_sha == current_head_sha {
            return Ok(true);
        }

        let output = Command::new("git")
            .current_dir(site_path)
            .args([
                "diff",
                "--name-only",
                cached_head_sha,
                current_head_sha,
                "--",
                file_path,
            ])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let diff_output = String::from_utf8_lossy(&out.stdout);
                Ok(diff_output.trim().is_empty())
            }
            _ => Ok(false),
        }
    }
}

fn migrate_project_paths(conn: &Connection) -> Result<(), rusqlite::Error> {
    for table in ["field_notes", "finding_cache", "workspace_sites"] {
        let has_project_path = conn
            .prepare(&format!("PRAGMA table_info({table})"))?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .any(|column| column == "project_path");

        if !has_project_path {
            conn.execute(
                &format!("ALTER TABLE {table} ADD COLUMN project_path TEXT"),
                [],
            )?;
        }
        conn.execute(
            &format!(
                "UPDATE {table} SET project_path = site_path \
                 WHERE project_path IS NULL OR project_path = ''"
            ),
            [],
        )?;
        conn.execute(
            &format!(
                "CREATE INDEX IF NOT EXISTS idx_{table}_project_path ON {table}(project_path)"
            ),
            [],
        )?;
    }
    Ok(())
}
