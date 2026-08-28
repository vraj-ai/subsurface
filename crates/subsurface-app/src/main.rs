#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use tauri::State;

use subsurface_engine::evidence::LineRange;
use subsurface_engine::grade::{Grade, LetterGrade};
use subsurface_engine::excavate::{excavate, provider_prompt, Finding};
use subsurface_engine::mcp::{McpServer, McpStatus};
use subsurface_engine::provider::{
    ConsentDecision, ConsentProvider, FakeProvider, KeychainStore, OpenAICompatibleProvider,
    OutboundPolicy, PayloadPreview, Provider, ProviderError, ALL_PRESETS, PRESET_OPENAI,
};
use subsurface_engine::project::Project;
use subsurface_engine::report::{estimate_site_report_cost, generate_site_report, SiteReport, SiteReportEstimate};
use subsurface_engine::site::{RecentSitesStore, Site};
use subsurface_engine::staleness::{detect_staleness, StalenessStatus};
use subsurface_engine::store::{ActivityRecord, ActivityStatus, FieldNote, SqliteStore};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRosterRow {
    pub name: String,
    pub path: String,
    pub quality_grade: String,
    pub last_assessment: String,
    pub in_flight_activity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectPickerData {
    pub projects: Vec<ProjectRosterRow>,
    pub scan_roots: Vec<String>,
}


pub struct AppState {
    pub store: Arc<SqliteStore>,
    pub mcp: Arc<McpServer>,
    pub recent_sites: Arc<Mutex<RecentSitesStore>>,
    pub settings: Arc<Mutex<ProviderSettings>>,
    pub outbound_policy: Arc<Mutex<OutboundPolicy>>,
}

fn get_db_path() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("subsurface").join("subsurface.db")
}

#[tauri::command]
fn open_project(path: String, state: State<'_, AppState>) -> Result<Project, String> {
    open_project_inner(&path, &state)
}

#[tauri::command]
fn open_site(path: String, state: State<'_, AppState>) -> Result<Site, String> {
    open_project_inner(&path, &state)
}

fn open_project_inner(path: &str, state: &AppState) -> Result<Project, String> {
    let p = PathBuf::from(path);
    let project = Project::open(&p).map_err(|e| e.to_string())?;
    let activity_id = state
        .store
        .record_activity(&project.root_path, "project_open", "Open Project")
        .ok();
    if let Some(id) = activity_id.as_deref() {
        let _ = state
            .store
            .update_activity(id, ActivityStatus::Running, None);
    }

    state
        .recent_sites
        .lock()
        .unwrap()
        .add(project.root_path.clone());

    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "repo".to_string());
    let _ = state.store.record_site_opened(&project.root_path, &name);
    if let Some(id) = activity_id.as_deref() {
        let _ = state
            .store
            .update_activity(id, ActivityStatus::Succeeded, None);
    }

    Ok(project)
}

#[tauri::command]
fn list_project_activities(
    project_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<ActivityRecord>, String> {
    state
        .store
        .list_activities(&PathBuf::from(project_path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn cancel_project_activity(
    activity_id: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    state
        .store
        .update_activity(&activity_id, ActivityStatus::Cancelled, Some("Cancelled"))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_home_workspace(state: State<'_, AppState>) -> Result<HomeWorkspaceData, String> {
    load_home_workspace(&state)
}

fn load_home_workspace(state: &AppState) -> Result<HomeWorkspaceData, String> {
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

fn format_overall_grade(grade: Grade) -> String {
    match grade {
        Grade::Incomplete => "Incomplete".to_string(),
        Grade::Letter(LetterGrade::APlus) => "A+".to_string(),
        Grade::Letter(LetterGrade::A) => "A".to_string(),
        Grade::Letter(LetterGrade::B) => "B".to_string(),
        Grade::Letter(LetterGrade::C) => "C".to_string(),
        Grade::Letter(LetterGrade::D) => "D".to_string(),
        Grade::Letter(LetterGrade::F) => "F".to_string(),
    }
}

fn roster_row_for(info: &DiscoveredSiteInfo, store: &SqliteStore) -> ProjectRosterRow {
    let assessments = store.list_assessments(&info.path).unwrap_or_default();
    let (quality_grade, last_assessment) = match assessments.first() {
        Some(assessment) => (
            format_overall_grade(assessment.grade.overall),
            assessment.assessed_at.clone(),
        ),
        None => ("Incomplete".to_string(), "Never assessed".to_string()),
    };
    let in_flight_activity = store
        .list_activities(&info.path)
        .unwrap_or_default()
        .into_iter()
        .find(|activity| {
            matches!(activity.status, ActivityStatus::Queued | ActivityStatus::Running)
        })
        .map(|activity| activity.title)
        .unwrap_or_else(|| "None".to_string());
    ProjectRosterRow {
        name: info.name.clone(),
        path: info.path.to_string_lossy().to_string(),
        quality_grade,
        last_assessment,
        in_flight_activity,
    }
}

#[tauri::command]
fn list_picker_projects(state: State<'_, AppState>) -> Result<ProjectPickerData, String> {
    let home = load_home_workspace(&state)?;
    let mut seen = HashSet::new();
    let mut projects = Vec::new();
    for info in home
        .pinned_sites
        .iter()
        .chain(home.recent_sites.iter())
        .chain(home.discovered_sites.iter())
    {
        if !seen.insert(info.path.clone()) {
            continue;
        }
        projects.push(roster_row_for(info, &state.store));
    }
    Ok(ProjectPickerData {
        projects,
        scan_roots: home.scan_roots,
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
    state: State<'_, AppState>,
) -> Result<PayloadPreview, String> {
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

    let full = fs::read_to_string(site.root_path.join(&file_path)).unwrap_or_default();
    let lines: Vec<&str> = full.lines().collect();
    let start_idx = start_line.saturating_sub(1);
    let end_idx = end_line.min(lines.len());
    let current_code = if start_idx < end_idx && start_idx < lines.len() {
        lines[start_idx..end_idx].join("\n")
    } else {
        String::new()
    };
    let prompt = provider_prompt(&current_code, &budgeted.included);
    let settings = state.settings.lock().map_err(|_| "Settings lock failed")?.clone();
    let provider = OpenAICompatibleProvider::new(settings.base_url, "", settings.model);
    state
        .outbound_policy
        .lock()
        .map_err(|_| "Outbound policy lock failed".to_string())
        .map(|policy| policy.preview(&provider, &site.root_path, &prompt))
}

fn provider_for_project(
    settings: &ProviderSettings,
    project_path: &std::path::Path,
    consent: Option<ConsentDecision>,
    state: &AppState,
) -> Arc<dyn Provider> {
    let key = if settings.offline_mode {
        String::new()
    } else {
        KeychainStore::get_key("subsurface_api_key").unwrap_or_default().unwrap_or_default()
    };
    if settings.offline_mode {
        return Arc::new(FakeProvider::new("Offline mode active: showing local git archaeology facts."));
    }
    if key.is_empty()
        && !settings.base_url.contains("localhost")
        && !settings.base_url.contains("127.0.0.1")
    {
        return Arc::new(FakeProvider::new(
            "No provider key configured in settings; showing git archaeology facts.",
        ));
    }
    let provider: Arc<dyn Provider> = Arc::new(OpenAICompatibleProvider::new(
        &settings.base_url,
        key,
        &settings.model,
    ));
    Arc::new(ConsentProvider::new(
        provider,
        state.outbound_policy.clone(),
        project_path,
        consent,
    ))
}

#[tauri::command]
fn excavate_range(
    site_path: String,
    file_path: String,
    start_line: usize,
    end_line: usize,
    consent: Option<ConsentDecision>,
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
    let provider = provider_for_project(&settings, &site.root_path, consent, &state);

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
    consent: Option<ConsentDecision>,
    state: State<'_, AppState>,
) -> Result<SiteReport, String> {
    assess_project_inner(&site_path, filter_prefix.as_deref(), consent, &state)
}

#[tauri::command]
fn assess_project(
    project_path: String,
    filter_prefix: Option<String>,
    consent: Option<ConsentDecision>,
    state: State<'_, AppState>,
) -> Result<SiteReport, String> {
    assess_project_inner(&project_path, filter_prefix.as_deref(), consent, &state)
}

fn assess_project_inner(
    project_path: &str,
    filter_prefix: Option<&str>,
    consent: Option<ConsentDecision>,
    state: &AppState,
) -> Result<SiteReport, String> {
    let project = Project::open(PathBuf::from(project_path)).map_err(|e| e.to_string())?;
    let activity_id = state
        .store
        .record_activity(&project.root_path, "assessment", "Assess Project")
        .ok();
    if let Some(id) = activity_id.as_deref() {
        let _ = state
            .store
            .update_activity(id, ActivityStatus::Running, None);
    }
    let settings = state.settings.lock().unwrap().clone();
    let provider = provider_for_project(&settings, &project.root_path, consent, state);

    match generate_site_report(&project, filter_prefix, provider).map_err(|e| e.to_string()) {
        Ok(report) => {
            if let Some(id) = activity_id.as_deref() {
                let _ = state
                    .store
                    .update_activity(id, ActivityStatus::Succeeded, None);
            }
            Ok(report)
        }
        Err(error) => {
            if let Some(id) = activity_id.as_deref() {
                let _ = state
                    .store
                    .update_activity(id, ActivityStatus::Failed, Some(&error));
            }
            Err(error)
        }
    }
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
    {
        let mut settings = state.settings.lock().unwrap();
        settings.base_url = base_url;
        settings.model = model;
        settings.offline_mode = offline_mode;
        settings.has_key = KeychainStore::get_key("subsurface_api_key").unwrap_or_default().is_some();
    }
    state
        .outbound_policy
        .lock()
        .map_err(|_| "Outbound policy lock failed".to_string())?
        .set_offline(offline_mode);
    Ok(())
}

#[tauri::command]
fn test_provider_connection(
    base_url: String,
    api_key: Option<String>,
    model: String,
    offline_mode: bool,
    state: State<'_, AppState>,
) -> Result<String, String> {
    if offline_mode || state.settings.lock().map_err(|_| "Settings lock failed")?.offline_mode {
        return Err(ProviderError::Offline.to_string());
    }
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
        outbound_policy: Arc::new(Mutex::new(OutboundPolicy::new(false))),
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            open_project,
            open_site,
            list_project_activities,
            cancel_project_activity,
            get_home_workspace,
            list_picker_projects,
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
            assess_project,
            generate_report,
            estimate_report,
            get_provider_settings,
            save_provider_settings,
            test_provider_connection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use subsurface_engine::fixture::GitFixture;

    fn test_state() -> AppState {
        let store = Arc::new(SqliteStore::in_memory().expect("store"));
        AppState {
            mcp: Arc::new(McpServer::new(
                store.clone(),
                Arc::new(FakeProvider::new("test")),
            )),
            store,
            recent_sites: Arc::new(Mutex::new(RecentSitesStore::default())),
            settings: Arc::new(Mutex::new(ProviderSettings {
                base_url: String::new(),
                model: String::new(),
                has_key: false,
                offline_mode: true,
                presets: Vec::new(),
            })),
            outbound_policy: Arc::new(Mutex::new(OutboundPolicy::new(true))),
        }
    }

    #[test]
    fn project_workflows_record_terminal_activities() {
        let mut project = GitFixture::new();
        project.commit("initial", &[("src/lib.rs", "fn example() {}\n")]);
        let state = test_state();

        let opened =
            open_project_inner(project.path().to_str().unwrap(), &state).expect("open project");
        assess_project_inner(project.path().to_str().unwrap(), None, None, &state)
            .expect("assess project");

        let activities = state
            .store
            .list_activities(&opened.root_path)
            .expect("activities");
        assert_eq!(activities.len(), 2);
        assert!(activities
            .iter()
            .all(|activity| activity.status == ActivityStatus::Succeeded));

        let empty_project = GitFixture::new();
        let empty_project_path = Project::open(empty_project.path())
            .expect("empty project")
            .root_path;
        let error = assess_project_inner(empty_project.path().to_str().unwrap(), None, None, &state)
            .expect_err("assessment without HEAD");
        let failed = state
            .store
            .list_activities(&empty_project_path)
            .expect("failed activity");
        assert_eq!(failed[0].status, ActivityStatus::Failed);
        assert_eq!(failed[0].detail.as_deref(), Some(error.as_str()));
    }
}
