#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use tauri::State;

use subsurface_engine::evidence::LineRange;
use subsurface_engine::excavate::{excavate, Finding};
use subsurface_engine::mcp::{McpServer, McpStatus};
use subsurface_engine::provider::{
    FakeProvider, KeychainStore, OpenAICompatibleProvider, Provider, ALL_PRESETS,
    PRESET_OPENAI,
};
use subsurface_engine::report::{estimate_site_report_cost, generate_site_report, SiteReport, SiteReportEstimate};
use subsurface_engine::site::{RecentSitesStore, Site};
use subsurface_engine::staleness::{detect_staleness, StalenessStatus};
use subsurface_engine::store::{FieldNote, SqliteStore};
use subsurface_engine::workspace::{discover_git_repositories, get_default_scan_roots, DiscoveredSiteInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetInfo {
    pub name: String,
    pub default_base_url: String,
    pub suggested_models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSettings {
    pub base_url: String,
    pub model: String,
    pub has_key: bool,
    pub offline_mode: bool,
    pub presets: Vec<PresetInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeWorkspaceData {
    pub pinned_sites: Vec<DiscoveredSiteInfo>,
    pub recent_sites: Vec<DiscoveredSiteInfo>,
    pub discovered_sites: Vec<DiscoveredSiteInfo>,
    pub scan_roots: Vec<String>,
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

    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "repo".to_string());
    let _ = state.store.record_site_opened(&site.root_path, &name);

    Ok(site)
}

#[tauri::command]
fn get_home_workspace(state: State<'_, AppState>) -> Result<HomeWorkspaceData, String> {
    let workspace_records = state.store.list_workspace_sites().unwrap_or_default();
    let custom_scan_roots = state.store.list_scan_roots().unwrap_or_default();
    let mut all_scan_roots = get_default_scan_roots();
    for cr in custom_scan_roots {
        if !all_scan_roots.contains(&cr) {
            all_scan_roots.push(cr);
        }
    }

    let discovered_raw = discover_git_repositories(&all_scan_roots, 3);

    let mut pinned_sites = Vec::new();
    let mut recent_sites = Vec::new();
    let mut discovered_sites = Vec::new();

    let pinned_paths: Vec<PathBuf> = workspace_records
        .iter()
        .filter(|w| w.is_pinned)
        .map(|w| w.site_path.clone())
        .collect();

    for d in discovered_raw {
        let notes_count = state.store.count_field_notes(&d.path).unwrap_or(0);
        let mut info = d;
        info.field_notes_count = notes_count;
        info.is_pinned = pinned_paths.contains(&info.path);

        if info.is_pinned {
            pinned_sites.push(info.clone());
        }

        let is_recent = workspace_records.iter().any(|r| r.site_path == info.path);
        if is_recent {
            recent_sites.push(info.clone());
        } else {
            discovered_sites.push(info);
        }
    }

    Ok(HomeWorkspaceData {
        pinned_sites,
        recent_sites,
        discovered_sites,
        scan_roots: all_scan_roots.iter().map(|p| p.to_string_lossy().to_string()).collect(),
    })
}

#[tauri::command]
fn toggle_pin_site(site_path: String, state: State<'_, AppState>) -> Result<bool, String> {
    let p = PathBuf::from(&site_path);
    state.store.toggle_pin_site(&p).map_err(|e| e.to_string())
}

#[tauri::command]
fn add_scan_root(path: String, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let p = PathBuf::from(&path);
    state.store.add_scan_root(&p).map_err(|e| e.to_string())?;
    let mut roots = get_default_scan_roots();
    for r in state.store.list_scan_roots().unwrap_or_default() {
        if !roots.contains(&r) {
            roots.push(r);
        }
    }
    Ok(roots.iter().map(|r| r.to_string_lossy().to_string()).collect())
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
    
    let provider: Arc<dyn Provider> = if settings.offline_mode {
        Arc::new(FakeProvider::new("Offline mode active: showing local git archaeology facts."))
    } else if !key.is_empty() || settings.base_url.contains("localhost") || settings.base_url.contains("127.0.0.1") {
        Arc::new(OpenAICompatibleProvider::new(&settings.base_url, &key, &settings.model))
    } else {
        Arc::new(FakeProvider::new("No provider key configured in settings; showing git archaeology facts."))
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
    Ok(detect_staleness(&site, &finding.file_path, &finding.what_when.timeline))
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
    let provider: Arc<dyn Provider> = if settings.offline_mode {
        Arc::new(FakeProvider::new("Site report offline pass"))
    } else if !key.is_empty() || settings.base_url.contains("localhost") {
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
fn get_provider_settings(state: State<'_, AppState>) -> Result<ProviderSettings, String> {
    let mut settings = state.settings.lock().unwrap().clone();
    settings.has_key = KeychainStore::get_key("subsurface_api_key").unwrap_or_default().is_some();
    Ok(settings)
}

#[tauri::command]
fn save_provider_settings(
    base_url: String,
    model: String,
    api_key: Option<String>,
    offline_mode: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(k) = api_key {
        let trimmed = k.trim();
        if !trimmed.is_empty() {
            KeychainStore::save_key("subsurface_api_key", trimmed)?;
        }
    }
    let mut settings = state.settings.lock().unwrap();
    settings.base_url = base_url;
    settings.model = model;
    settings.offline_mode = offline_mode;
    settings.has_key = KeychainStore::get_key("subsurface_api_key").unwrap_or_default().is_some();
    Ok(())
}

#[tauri::command]
fn test_provider_connection(
    base_url: String,
    api_key: Option<String>,
    model: String,
) -> Result<String, String> {
    let key = if let Some(k) = api_key {
        if !k.trim().is_empty() {
            k
        } else {
            KeychainStore::get_key("subsurface_api_key").unwrap_or_default().unwrap_or_default()
        }
    } else {
        KeychainStore::get_key("subsurface_api_key").unwrap_or_default().unwrap_or_default()
    };

    let provider = OpenAICompatibleProvider::new(base_url, key, model);
    provider
        .complete("Ping. Respond with 'OK'.")
        .map_err(|e| e.to_string())
}

fn main() {
    let db_path = get_db_path();
    let store = Arc::new(SqliteStore::open(db_path).unwrap_or_else(|_| SqliteStore::in_memory().unwrap()));
    let fake_provider = Arc::new(FakeProvider::new("Subsurface desktop engine"));
    let mcp = Arc::new(McpServer::new(store.clone(), fake_provider));

    let presets = ALL_PRESETS
        .iter()
        .map(|p| PresetInfo {
            name: p.name.to_string(),
            default_base_url: p.default_base_url.to_string(),
            suggested_models: p.suggested_models.iter().map(|s| s.to_string()).collect(),
        })
        .collect();

    let initial_settings = ProviderSettings {
        base_url: PRESET_OPENAI.default_base_url.to_string(),
        model: "gpt-4o".to_string(),
        has_key: KeychainStore::get_key("subsurface_api_key").unwrap_or_default().is_some(),
        offline_mode: false,
        presets,
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
            get_home_workspace,
            toggle_pin_site,
            add_scan_root,
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
            get_provider_settings,
            save_provider_settings,
            test_provider_connection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
