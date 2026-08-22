use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::evidence::LineRange;
use crate::excavate::{excavate, Finding};
use crate::provider::Provider;
use crate::site::Site;
use crate::store::{FieldNote, SqliteStore};

#[derive(Debug, Error)]
pub enum McpError {
    #[error("Subsurface is not running or store is unavailable")]
    NotRunning,
    #[error("Failed to open Site at '{0}': {1}")]
    SiteOpen(String, String),
    #[error("Excavate error: {0}")]
    Excavate(String),
    #[error("Store error: {0}")]
    Store(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpToolCall {
    Excavate {
        site_path: String,
        file_path: String,
        start_line: usize,
        end_line: usize,
    },
    ListFieldNotes {
        site_path: String,
    },
    GetFieldNote {
        id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpExcavateResult {
    pub finding: Finding,
    pub from_cached_field_note: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAuditEntry {
    pub timestamp: String,
    pub tool_name: String,
    pub query_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpStatus {
    pub is_running: bool,
    pub active_queries_count: usize,
    pub recent_agent_queries: Vec<AgentAuditEntry>,
}

pub struct McpServer {
    store: Arc<SqliteStore>,
    provider: Arc<dyn Provider>,
    audit_log: Arc<Mutex<Vec<AgentAuditEntry>>>,
}

impl McpServer {
    pub fn new(store: Arc<SqliteStore>, provider: Arc<dyn Provider>) -> Self {
        Self {
            store,
            provider,
            audit_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn status(&self) -> McpStatus {
        let logs = self.audit_log.lock().unwrap().clone();
        McpStatus {
            is_running: true,
            active_queries_count: logs.len(),
            recent_agent_queries: logs,
        }
    }

    pub fn handle_tool_call(&self, call: McpToolCall) -> Result<McpExcavateResult, McpError> {
        match call {
            McpToolCall::Excavate {
                site_path,
                file_path,
                start_line,
                end_line,
            } => {
                let site_buf = PathBuf::from(&site_path);
                let range = LineRange {
                    start: start_line,
                    end: end_line,
                };

                // Record audit log
                {
                    let mut logs = self.audit_log.lock().unwrap();
                    logs.push(AgentAuditEntry {
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        tool_name: "excavate".to_string(),
                        query_summary: format!("{}:{}-{}", file_path, start_line, end_line),
                    });
                    if logs.len() > 50 {
                        logs.remove(0);
                    }
                }

                // 1. Check if a saved Field Note already exists for this selection
                let existing_notes = self
                    .store
                    .list_field_notes(&site_buf)
                    .map_err(|e| McpError::Store(e.to_string()))?;

                for note in existing_notes {
                    if note.file_path == file_path && note.line_range == range {
                        return Ok(McpExcavateResult {
                            finding: note.finding,
                            from_cached_field_note: true,
                        });
                    }
                }

                // 2. Otherwise run excavate() and save to shared Field Notes
                let site = Site::open(&site_buf)
                    .map_err(|e| McpError::SiteOpen(site_path.clone(), e.to_string()))?;

                let finding = excavate(&site, &file_path, range, self.provider.clone())
                    .map_err(|e| McpError::Excavate(e.to_string()))?;

                let _ = self.store.save_field_note(
                    &site_buf,
                    &finding,
                    "Excavated via MCP agent query",
                );

                Ok(McpExcavateResult {
                    finding,
                    from_cached_field_note: false,
                })
            }
            _ => Err(McpError::Excavate("Unsupported tool call in excavate handler".into())),
        }
    }

    pub fn list_field_notes(&self, site_path: &str) -> Result<Vec<FieldNote>, McpError> {
        let path = PathBuf::from(site_path);
        self.store
            .list_field_notes(&path)
            .map_err(|e| McpError::Store(e.to_string()))
    }
}
