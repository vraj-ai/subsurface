#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use tauri::State;

use subsurface_engine::evidence::LineRange;
use subsurface_engine::excavate::{excavate, Finding};
use subsurface_engine::mcp::{McpServer, McpStatus};
use subsurface_engine::provider::{FakeProvider, KeychainStore, OpenAICompatibleProvider, Provider, PRESET_OPENAI};
use subsurface_engine::report::{estimate_site_report_cost, generate_site_report, SiteReport, SiteReportEstimate};
use subsurface_engine::site::{RecentSitesStore, Site};
use subsurface_engine::staleness::{detect_staleness, StalenessStatus};
use subsurface_engine::store::{FieldNote, SqliteStore};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSettings {
    pub base_url: String,
    pub model: String,
    pub has_key: bool,
}

pub struct AppState {
    pub store: Arc<SqliteStore>,
    pub mcp: Arc<McpServer>,
    pub recent_sites: Arc<Mutex<RecentSitesStore>>,
    pub settings: Arc<Mutex<ProviderSettings>>,
}

fn get_db_path() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("subsurface").join("subsurface.db")
}

#[tauri::command]
fn open_site(path: String, state: State<'_, AppState>) -> Result<Site, String> {
    let p = PathBuf::from(&path);
    let site = Site::open(&p).map_err(|e| e.to_string())?;

    let mut recents = state.recent_sites.lock().unwrap();
    recents.add(site.root_path.clone());

    Ok(site)
}

#[tauri::command]
fn list_recent_sites(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let recents = state.recent_sites.lock().unwrap();
    Ok(recents.sites.iter().map(|p| p.to_string_lossy().to_string()).collect())
}

#[tauri::command]
fn read_file_content(site_path: String, rel_path: String) -> Result<String, String> {
    let root = PathBuf::from(site_path);
    let full = root.join(rel_path);
    fs::read_to_string(&full).map_err(|e| e.to_string())
}

#[tauri::command]
fn preview_payload(
    site_path: String,
    file_path: String,
    start_line: usize,
    end_line: usize,
) -> Result<String, String> {
    let site = Site::open(PathBuf::from(&site_path)).map_err(|e| e.to_string())?;
    let range = LineRange {
        start: start_line,
        end: end_line,
    };
    let evidence = subsurface_engine::evidence::walk_evidence(&site, &file_path, range)
        .map_err(|e| e.to_string())?;
    let budgeted = subsurface_engine::budget::rank_and_budget_evidence(
        &evidence,
        &subsurface_engine::budget::BudgetConfig::default(),
    );

    let mut payload = format!(
        "--- TARGET CODE ({}:{}-{}) ---\n\n",
        file_path, start_line, end_line
    );
    let full = fs::read_to_string(site.root_path.join(&file_path)).unwrap_or_default();
    let lines: Vec<&str> = full.lines().collect();
    let start_idx = start_line.saturating_sub(1);
    let end_idx = end_line.min(lines.len());
    if start_idx < end_idx && start_idx < lines.len() {
        payload.push_str(&lines[start_idx..end_idx].join("\n"));
        payload.push_str("\n\n");
    }

    payload.push_str("--- INCLUDED EVIDENCE (Ranked) ---\n\n");
    for (i, ev) in budgeted.included.iter().enumerate() {
        payload.push_str(&format!(
            "[{}] Commit {} by {} ({})\nMessage: {}\nDiff:\n{}\n\n",
            i + 1,
            &ev.commit_sha[..7.min(ev.commit_sha.len())],
            ev.author,
            ev.timestamp,
            ev.message,
            ev.diff
        ));
    }

    payload.push_str(&format!(
        "--- EXCLUDED EVIDENCE ({} items beyond budget cap) ---\n\n",
        budgeted.excluded.len()
    ));
    for ex in &budgeted.excluded {
        payload.push_str(&format!(
            "- Commit {}: {} ({})\n",
            &ex.commit_sha[..7.min(ex.commit_sha.len())],
            ex.message_summary,
            ex.reason
        ));
    }

    Ok(payload)
}

#[tauri::command]
fn excavate_range(
    site_path: String,
    file_path: String,
    start_line: usize,
    end_line: usize,
    state: State<'_, AppState>,
) -> Result<Finding, String> {
    let site = Site::open(PathBuf::from(&site_path)).map_err(|e| e.to_string())?;
    let range = LineRange {
        start: start_line,
        end: end_line,
    };

    if let Some(ref head_sha) = site.head_commit {
        if let Ok(Some(cached)) = state.store.get_cached_finding(&site.root_path, &file_path, range, head_sha) {
            return Ok(cached);
        }
    }

    let settings = state.settings.lock().unwrap().clone();
    let key = KeychainStore::get_key("subsurface_api_key").unwrap_or_default().unwrap_or_default();
    let provider: Arc<dyn Provider> = if !key.is_empty() {
        Arc::new(OpenAICompatibleProvider::new(&settings.base_url, &key, &settings.model))
    } else {
        Arc::new(FakeProvider::new("No provider key configured; showing git archaeology facts."))
    };

    let finding = excavate(&site, &file_path, range, provider).map_err(|e| e.to_string())?;

    if let Some(ref head_sha) = site.head_commit {
        let _ = state.store.cache_finding(&site.root_path, &file_path, range, head_sha, &finding);
    }

    Ok(finding)
}

#[tauri::command]
fn check_staleness(site_path: String, finding: Finding) -> Result<StalenessStatus, String> {
    let site = Site::open(PathBuf::from(&site_path)).map_err(|e| e.to_string())?;
    Ok(detect_staleness(&site, &finding))
}

#[tauri::command]
fn save_field_note(
    site_path: String,
    finding: Finding,
    user_notes: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let p = PathBuf::from(&site_path);
    state.store.save_field_note(&p, &finding, &user_notes).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_field_notes(site_path: String, state: State<'_, AppState>) -> Result<Vec<FieldNote>, String> {
    let p = PathBuf::from(&site_path);
    state.store.list_field_notes(&p).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_field_note(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    state.store.delete_field_note(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_mcp_status(state: State<'_, AppState>) -> Result<McpStatus, String> {
    Ok(state.mcp.status())
}

#[tauri::command]
fn generate_report(
    site_path: String,
    filter_prefix: Option<String>,
    state: State<'_, AppState>,
) -> Result<SiteReport, String> {
    let site = Site::open(PathBuf::from(&site_path)).map_err(|e| e.to_string())?;
    let settings = state.settings.lock().unwrap().clone();
    let key = KeychainStore::get_key("subsurface_api_key").unwrap_or_default().unwrap_or_default();
    let provider: Arc<dyn Provider> = if !key.is_empty() {
        Arc::new(OpenAICompatibleProvider::new(&settings.base_url, &key, &settings.model))
    } else {
        Arc::new(FakeProvider::new("Site report archaeology pass"))
    };

    generate_site_report(&site, filter_prefix.as_deref(), provider).map_err(|e| e.to_string())
}

#[tauri::command]
fn estimate_report(site_path: String, filter_prefix: Option<String>) -> Result<SiteReportEstimate, String> {
    let site = Site::open(PathBuf::from(&site_path)).map_err(|e| e.to_string())?;
    Ok(estimate_site_report_cost(&site, filter_prefix.as_deref()))
}

#[tauri::command]
fn save_provider_settings(
    base_url: String,
    model: String,
    api_key: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(k) = api_key {
        if !k.is_empty() {
            KeychainStore::save_key("subsurface_api_key", &k)?;
        }
    }
    let mut settings = state.settings.lock().unwrap();
    settings.base_url = base_url;
    settings.model = model;
    settings.has_key = KeychainStore::get_key("subsurface_api_key").unwrap_or_default().is_some();
    Ok(())
}

fn main() {
    let db_path = get_db_path();
    let store = Arc::new(SqliteStore::open(db_path).unwrap_or_else(|_| SqliteStore::in_memory().unwrap()));
    let fake_provider = Arc::new(FakeProvider::new("Subsurface desktop engine"));
    let mcp = Arc::new(McpServer::new(store.clone(), fake_provider));

    let initial_settings = ProviderSettings {
        base_url: PRESET_OPENAI.default_base_url.to_string(),
        model: "gpt-4o".to_string(),
        has_key: KeychainStore::get_key("subsurface_api_key").unwrap_or_default().is_some(),
    };

    let app_state = AppState {
        store,
        mcp,
        recent_sites: Arc::new(Mutex::new(RecentSitesStore::default())),
        settings: Arc::new(Mutex::new(initial_settings)),
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            open_site,
            list_recent_sites,
            read_file_content,
            preview_payload,
            excavate_range,
            check_staleness,
            save_field_note,
            list_field_notes,
            delete_field_note,
            get_mcp_status,
            generate_report,
            estimate_report,
            save_provider_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
