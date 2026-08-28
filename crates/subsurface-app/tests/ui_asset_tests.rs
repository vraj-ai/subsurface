use std::collections::HashMap;
use std::path::Path;

const UI_BUNDLE: &str = include_str!("../ui/index.html");
const TOKENS_CSS: &str = include_str!("../ui/tokens.css");
const VISUAL_FIXTURE: &str = include_str!("../ui/fixtures/visual.html");

const SPEC_ORDER: [&str; 8] = [
    "problem",
    "finding",
    "evidence",
    "candidate",
    "checks",
    "grade",
    "receipt",
    "publish",
];

const SPEC_LABELS: [&str; 8] = [
    "Problem",
    "Finding",
    "Evidence",
    "Candidate",
    "Checks",
    "Grade",
    "Receipt",
    "Publish",
];

const SETTINGS_SECTIONS: [&str; 5] = [
    "connections",
    "privacy",
    "quality",
    "automation",
    "appearance",
];

const SETTINGS_LABELS: [&str; 5] = [
    "Connections",
    "Privacy",
    "Quality",
    "Automation",
    "Appearance",
];

const TEXT_PAIRS: &[(&str, &str)] = &[
    ("text-primary", "bg-dark"),
    ("text-primary", "bg-panel"),
    ("text-primary", "bg-card"),
    ("text-secondary", "bg-dark"),
    ("text-secondary", "bg-panel"),
    ("text-muted", "bg-dark"),
    ("text-muted", "bg-panel"),
    ("text-on-accent", "accent-blue-btn"),
    ("status-ok-fg", "status-ok-bg"),
    ("status-warn-fg", "status-warn-bg"),
    ("status-fault-fg", "status-fault-bg"),
];

const UI_PAIRS: &[(&str, &str)] = &[("focus-ring", "bg-dark")];

#[test]
fn ui_asset_tests_harness_reads_ui_bundle() {
    let bundle_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/index.html");
    assert!(
        bundle_path.is_file(),
        "UI bundle is missing at {}",
        bundle_path.display()
    );

    assert!(
        !UI_BUNDLE.trim().is_empty(),
        "UI bundle is present but empty"
    );

    let html = UI_BUNDLE.to_ascii_lowercase();
    assert!(
        html.contains("<!doctype html>"),
        "UI bundle is not parseable HTML (missing doctype)"
    );
    assert!(
        html.contains("<html") && html.contains("</html>"),
        "UI bundle is not parseable HTML (missing html root)"
    );
    assert!(
        html.contains("<body") && html.contains("</body>"),
        "UI bundle is not parseable HTML (missing body root)"
    );
}

#[test]
fn detail_surface_section_order_matches_spec() {
    let strata = extract_strata_surface(UI_BUNDLE);
    assert!(
        !strata.trim().is_empty(),
        "Opportunity detail Strata is missing from the UI bundle"
    );

    assert!(
        strata_uses_column_css(UI_BUNDLE),
        "detail surface must be a stacked column, not a tab strip"
    );
    assert!(
        !contains_tab_pattern(&strata),
        "Finding detail is Strata (stacked sequence, not tabs)"
    );

    let ids = attr_values(&strata, "data-strata-section");
    assert_eq!(
        ids,
        SPEC_ORDER,
        "detail surface section order must match Problem -> Finding -> Evidence -> Candidate -> Checks -> Grade -> Receipt -> Publish"
    );

    let labels = heading_labels(&strata);
    assert_eq!(
        labels, SPEC_LABELS,
        "visible Strata headings must follow the locked section order"
    );

    assert!(
        !UI_BUNDLE.contains("id=\"reportModal\""),
        "tabbed Site Report modal must not remain the Opportunity detail surface"
    );
    assert!(
        !UI_BUNDLE.contains("id=\"reportCategoryTabs\""),
        "Opportunity detail must not use category tabs"
    );
}

#[test]
fn settings_is_sheet_with_five_sections() {
    let settings = extract_surface(UI_BUNDLE, "settings");
    assert!(
        !settings.trim().is_empty(),
        "Settings sheet is missing from the UI bundle"
    );

    assert!(
        settings.contains("data-overlay=\"sheet\""),
        "Settings must be a sheet overlay, not a full-screen room"
    );
    assert!(
        !settings.contains("view-container"),
        "Settings is a sheet over the Shell, not a view-container room"
    );
    assert!(
        sheet_uses_overlay_css(UI_BUNDLE),
        "Settings sheet must use a fixed overlay so the Shell stays mounted"
    );
    assert!(
        css_block(UI_BUNDLE, ".settings-section").contains("display: block"),
        "Settings sections must be stacked blocks, not a tab strip"
    );
    assert!(
        !contains_tab_pattern(&settings),
        "Settings sections are stacked in the sheet, not tabs"
    );

    let ids = attr_values(&settings, "data-settings-section");
    assert_eq!(
        ids,
        SETTINGS_SECTIONS,
        "Settings section order must match Connections -> Privacy -> Quality -> Automation -> Appearance"
    );

    let labels = heading_labels(&settings);
    assert_eq!(
        labels, SETTINGS_LABELS,
        "visible Settings headings must name the five product sections"
    );

    let activity = extract_surface(UI_BUNDLE, "activity");
    assert!(
        !activity.trim().is_empty(),
        "Activity must stay mounted in the Shell when Settings is a sheet"
    );
    assert!(
        !settings.contains("data-surface=\"activity\""),
        "Activity stays mounted in the Shell, not inside the Settings sheet"
    );

    let payload = extract_surface(UI_BUNDLE, "payload");
    assert!(
        payload.contains("data-overlay=\"sheet\""),
        "payload preview must use the same sheet overlay pattern as Settings"
    );

    assert!(
        !UI_BUNDLE.contains("id=\"settingsModal\""),
        "provider Settings modal must not remain a full-screen Settings room"
    );
    assert!(
        !UI_BUNDLE.contains("id=\"settingsView\""),
        "Settings must not ship as a full-screen view"
    );
}

#[test]
fn tokens_css_defines_oklch_light_and_dark_and_passes_contrast_check() {
    let tokens_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/tokens.css");
    assert!(
        tokens_path.is_file(),
        "tokens.css is missing at {}",
        tokens_path.display()
    );

    assert!(
        UI_BUNDLE.contains("href=\"tokens.css\""),
        "the real UI bundle must load tokens.css"
    );

    assert!(
        TOKENS_CSS.contains("oklch("),
        "tokens.css must express color in OKLCH"
    );
    assert!(
        TOKENS_CSS.contains("[data-theme=\"light\"]"),
        "tokens.css must define a light climate"
    );
    assert!(
        TOKENS_CSS.contains("[data-theme=\"dark\"]"),
        "tokens.css must define a dark climate"
    );
    assert!(
        TOKENS_CSS.contains("prefers-color-scheme: dark"),
        "appearance must follow macOS by default"
    );
    assert!(
        TOKENS_CSS.contains("prefers-reduced-motion"),
        "reduced motion must be a complete experience"
    );
    assert!(
        TOKENS_CSS.contains(":focus-visible"),
        "visible focus is required"
    );
    assert!(
        TOKENS_CSS.contains("@font-face")
            && TOKENS_CSS.contains("url(\"fonts/")
            && !TOKENS_CSS.contains("http://")
            && !TOKENS_CSS.contains("https://"),
        "type must be bundled locally with no webfont CDN"
    );
    assert!(
        TOKENS_CSS.contains(".quality-rail") && TOKENS_CSS.contains(".in-flight"),
        "quality rail and in-flight motion must be tokenized"
    );

    let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/fonts");
    for name in [
        "instrument-sans-400.woff2",
        "instrument-sans-600.woff2",
        "martian-mono-400.woff2",
    ] {
        let path = fonts.join(name);
        assert!(path.is_file(), "bundled font missing: {}", path.display());
    }

    assert!(
        VISUAL_FIXTURE.contains("href=\"../tokens.css\""),
        "visual fixtures must load the same tokens.css"
    );
    assert!(
        VISUAL_FIXTURE.contains("quality-rail") && VISUAL_FIXTURE.contains("in-flight"),
        "visual fixtures must show the quality rail and in-flight stillness contrast"
    );
    assert!(
        !VISUAL_FIXTURE.to_ascii_lowercase().contains("<script"),
        "visual fixtures must stay dependency-free (no JS)"
    );

    let light = parse_oklch_vars(TOKENS_CSS, "light");
    let dark = parse_oklch_vars(TOKENS_CSS, "dark");
    assert!(
        light.len() >= 16,
        "light climate is missing OKLCH tokens, found {}",
        light.len()
    );
    assert!(
        dark.len() >= 16,
        "dark climate is missing OKLCH tokens, found {}",
        dark.len()
    );

    for climate in [("light", &light), ("dark", &dark)] {
        for (fg_name, bg_name) in TEXT_PAIRS {
            let ratio = contrast_pair(climate.1, fg_name, bg_name, climate.0);
            assert!(
                ratio >= 4.5,
                "{} {} on {} contrast {:.2} is below WCAG AA 4.5:1",
                climate.0,
                fg_name,
                bg_name,
                ratio
            );
        }
        for (fg_name, bg_name) in UI_PAIRS {
            let ratio = contrast_pair(climate.1, fg_name, bg_name, climate.0);
            assert!(
                ratio >= 3.0,
                "{} {} on {} contrast {:.2} is below WCAG UI 3:1",
                climate.0,
                fg_name,
                bg_name,
                ratio
            );
        }
    }
}

fn parse_oklch_vars(css: &str, climate: &str) -> HashMap<String, (f64, f64, f64)> {
    let prefix = format!("--{climate}-");
    let mut vars = HashMap::new();
    for raw_line in css.lines() {
        let line = raw_line.trim();
        let Some(rest) = line.strip_prefix(&prefix) else {
            continue;
        };
        let Some((name, value)) = rest.split_once(':') else {
            continue;
        };
        if let Some(oklch) = parse_oklch(value) {
            vars.insert(name.trim().to_string(), oklch);
        }
    }
    vars
}

fn parse_oklch(value: &str) -> Option<(f64, f64, f64)> {
    let start = value.find("oklch(")?;
    let inner = value[start + 6..].split(')').next()?;
    let inner = inner.split('/').next().unwrap_or(inner);
    let parts: Vec<&str> = inner.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }
    let l = parse_lightness(parts[0])?;
    let c = parts[1].parse().ok()?;
    let h = parts[2].parse().ok()?;
    Some((l, c, h))
}

fn parse_lightness(raw: &str) -> Option<f64> {
    if let Some(pct) = raw.strip_suffix('%') {
        return pct.parse::<f64>().ok().map(|v| v / 100.0);
    }
    raw.parse().ok()
}

fn contrast_pair(
    vars: &HashMap<String, (f64, f64, f64)>,
    fg_name: &str,
    bg_name: &str,
    climate: &str,
) -> f64 {
    let fg = vars
        .get(fg_name)
        .unwrap_or_else(|| panic!("missing --{climate}-{fg_name} oklch token"));
    let bg = vars
        .get(bg_name)
        .unwrap_or_else(|| panic!("missing --{climate}-{bg_name} oklch token"));
    contrast_ratio(*fg, *bg)
}

fn contrast_ratio(fg: (f64, f64, f64), bg: (f64, f64, f64)) -> f64 {
    let l1 = relative_luminance(fg);
    let l2 = relative_luminance(bg);
    let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

fn relative_luminance((l, c, h): (f64, f64, f64)) -> f64 {
    let (r, g, b) = oklch_to_linear_srgb(l, c, h);
    0.2126 * r.clamp(0.0, 1.0) + 0.7152 * g.clamp(0.0, 1.0) + 0.0722 * b.clamp(0.0, 1.0)
}

fn oklch_to_linear_srgb(l: f64, c: f64, h_deg: f64) -> (f64, f64, f64) {
    let h = h_deg.to_radians();
    let a = c * h.cos();
    let b = c * h.sin();
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548_0 * b;
    let l3 = l_.powi(3);
    let m3 = m_.powi(3);
    let s3 = s_.powi(3);
    let r = 4.076_741_662_1 * l3 - 3.307_711_591_3 * m3 + 0.230_969_929_2 * s3;
    let g = -1.268_438_004_6 * l3 + 2.609_757_401_1 * m3 - 0.341_319_396_5 * s3;
    let b = -0.004_196_086_3 * l3 - 0.703_418_614_7 * m3 + 1.707_614_701_0 * s3;
    (r, g, b)
}

#[test]
fn activity_center_survives_navigation() {
    let activity = extract_surface(UI_BUNDLE, "activity");
    assert!(
        !activity.trim().is_empty(),
        "Activity center is missing from the UI bundle"
    );
    assert!(
        activity.contains("activity-strip"),
        "Activity is the Shell's bottom strip"
    );
    assert!(
        !activity.contains("view-container"),
        "Activity is not a navigable view-container room"
    );

    let strip_css = css_block(UI_BUNDLE, ".activity-strip");
    assert!(
        strip_css.contains("position: fixed") && strip_css.contains("bottom: 0"),
        "Activity must be a persistent bottom strip"
    );
    assert!(
        strip_css.contains("display: flex") && !strip_css.contains("display: none"),
        "Activity stays mounted; it is not a hidden view"
    );

    let home_pos = UI_BUNDLE
        .find("id=\"homeView\"")
        .expect("home view missing");
    let code_pos = UI_BUNDLE
        .find("id=\"codeView\"")
        .expect("code view missing");
    let activity_pos = UI_BUNDLE
        .find("data-surface=\"activity\"")
        .expect("activity surface missing");
    assert!(
        activity_pos > home_pos && activity_pos > code_pos,
        "Activity is a Shell sibling after the views so navigation cannot unmount it"
    );

    let settings = extract_surface(UI_BUNDLE, "settings");
    assert!(
        !settings.contains("data-surface=\"activity\""),
        "Settings sheet does not unmount Activity"
    );
    let overlay_css = css_block(UI_BUNDLE, ".sheet-overlay");
    assert!(
        overlay_css.contains("bottom: var(--activity-strip-height)")
            || overlay_css.contains("z-index: 100"),
        "Settings sheet overlays the Shell without replacing Activity"
    );

    assert!(
        activity.contains("data-activity-list"),
        "Activity center lists durable work"
    );
    assert!(
        activity.contains("No in-flight work."),
        "empty Activity state must name that no work is in flight"
    );

    assert!(
        UI_BUNDLE.contains("list_project_activities"),
        "Activity center reads durable records from the engine store"
    );
    assert!(
        UI_BUNDLE.contains("function refreshActivityCenter")
            || UI_BUNDLE.contains("async function refreshActivityCenter"),
        "Activity center refreshes without being recreated"
    );
    assert!(
        UI_BUNDLE.contains("function renderActivityCenter"),
        "Activity center renders progress, receipts, and failures"
    );
    assert!(
        UI_BUNDLE.contains("function cancelActivity"),
        "cancellation is an explicit Activity action"
    );
    assert!(
        UI_BUNDLE.contains("cancel_project_activity"),
        "Activity cancel uses the durable store, not navigation"
    );

    for kind in ["assessment", "preparation", "verification", "publication"] {
        assert!(
            UI_BUNDLE.contains(&format!("\"{kind}\"")),
            "Activity center tracks {kind}"
        );
    }

    let renderer = js_function_body(UI_BUNDLE, "renderActivityCenter");
    for needle in [
        "queued",
        "running",
        "succeeded",
        "failed",
        "cancelled",
        "activity-progress",
        "activity-receipt",
    ] {
        assert!(
            renderer.contains(needle),
            "Activity center must show {needle}"
        );
    }

    for name in [
        "showHomeView",
        "showCodeView",
        "closeModal",
        "loadFile",
        "openSettingsModal",
    ] {
        let body = js_function_body(UI_BUNDLE, name);
        assert!(
            !body.contains("cancelActivity") && !body.contains("cancel_project_activity"),
            "{name} must not cancel work"
        );
        assert!(
            !body.contains("activityStrip")
                && !body.contains("activity-strip")
                && !body.contains("data-surface=\"activity\""),
            "{name} must not unmount or hide Activity"
        );
    }
}

#[test]
fn project_picker_is_roster_with_project_vocabulary() {
    assert!(
        UI_BUNDLE.contains("data-mode=\"picker\""),
        "launch must open in Project Picker mode before Shell chrome"
    );
    let picker_hide = css_block(UI_BUNDLE, "body[data-mode=\"picker\"] .shell-chrome");
    assert!(
        picker_hide.contains("display: none"),
        "Shell chrome must stay hidden while the Project Picker is the launch surface"
    );

    let picker = extract_surface(UI_BUNDLE, "picker");
    assert!(
        picker.contains("project-picker") && picker.contains("active"),
        "Launch shows the Project Picker before Shell chrome"
    );

    let picker_pos = UI_BUNDLE
        .find("data-surface=\"picker\"")
        .expect("picker surface missing");
    let shell_pos = UI_BUNDLE
        .find("id=\"codeView\"")
        .expect("Shell chrome missing");
    assert!(
        picker_pos < shell_pos,
        "Project Picker must appear before Shell chrome"
    );

    assert!(
        !picker.contains("project-grid") && !picker.contains("project-card"),
        "No card grid as the launch surface"
    );
    let roster_css = css_block(UI_BUNDLE, ".project-roster");
    assert!(
        roster_css.contains("flex-direction: column"),
        "Picker must be a dense roster, not a card grid"
    );
    assert!(
        !roster_css.contains("auto-fill"),
        "No card grid as the launch surface"
    );

    let fields = attr_values(&picker, "data-picker-field");
    assert_eq!(
        fields,
        [
            "name",
            "path",
            "quality-grade",
            "last-assessment",
            "in-flight-activity"
        ],
        "each roster row must show name, path, Quality Grade, last Assessment, and in-flight Activity"
    );
    for label in ["Quality Grade", "Assessment", "Activity"] {
        assert!(picker.contains(label), "roster must label {label}");
    }

    assert!(picker.contains("Project"), "active copy uses Project");
    assert!(
        !picker_mentions_site(&picker),
        "Site must not appear on the Project Picker surface"
    );

    let empty = extract_marked_element(&picker, "data-picker-state", "empty");
    let error = extract_marked_element(&picker, "data-picker-state", "error");
    assert!(
        empty.contains("empty-verb"),
        "empty state must use empty-verb"
    );
    assert!(
        error.contains("empty-verb"),
        "error state must use empty-verb"
    );
    assert!(
        names_next_verb(&empty),
        "empty state must name the next verb, got {empty}"
    );
    assert!(
        names_next_verb(&error),
        "error state must name the next verb, got {error}"
    );

    assert!(
        picker.contains("data-opens=\"shell\"")
            || UI_BUNDLE.contains("function openProjectFromRoster"),
        "choosing a roster row must open the Shell"
    );
}

#[test]
fn shell_has_quality_rail_opportunities_assessment_activity() {
    assert!(
        UI_BUNDLE.contains("data-mode=\"picker\""),
        "launch still opens the Project Picker before the Shell"
    );
    assert!(
        UI_BUNDLE.contains("function showCodeView")
            && (js_function_body(UI_BUNDLE, "showCodeView").contains("data-mode")
                || js_function_body(UI_BUNDLE, "showCodeView").contains("dataset.mode")),
        "opening a Project must enter one Shell mode"
    );
    let open_shell = js_function_body(UI_BUNDLE, "openProjectFromRoster");
    assert!(
        open_shell.contains("showCodeView") || open_shell.contains("showShell"),
        "choosing a Project must open the Shell"
    );

    assert!(
        !UI_BUNDLE.contains("Code Explorer"),
        "after open, one Shell — not Workspace vs Code Explorer"
    );
    assert!(
        !UI_BUNDLE.contains("Home Workspace"),
        "after open, one Shell — not Workspace vs Code Explorer"
    );
    assert!(
        !UI_BUNDLE.contains("id=\"btnNavCode\""),
        "Excavate is not a Code Explorer room tab"
    );
    let shell_css = css_block(UI_BUNDLE, ".shell-layout");
    assert!(
        shell_css.contains("grid-template-columns") && !shell_css.contains("display: none"),
        "the open Project is one Shell layout, not two rooms"
    );

    let header_end = UI_BUNDLE.find("</header>").expect("header missing");
    let rail_pos = UI_BUNDLE
        .find("data-surface=\"quality-rail\"")
        .expect("Quality Rail surface missing");
    assert!(
        rail_pos < header_end,
        "Quality Rail is a horizontal header instrument"
    );
    let rail = extract_surface(UI_BUNDLE, "quality-rail");
    assert!(
        rail.contains("quality-rail") && rail.contains("quality-rail-grade"),
        "Quality Rail must show the overall Quality Grade"
    );
    assert!(
        rail.contains("Incomplete"),
        "Quality Rail must show Incomplete when measurements are missing"
    );
    let dimensions = attr_values(&rail, "data-dimension");
    assert_eq!(
        dimensions,
        [
            "correctness",
            "tests",
            "security",
            "maintainability",
            "simplicity",
            "evidence"
        ],
        "Quality Rail must show the measured dimensions"
    );
    assert!(
        rail.contains("data-grade=\"incomplete\""),
        "missing dimensions stay visible as Incomplete"
    );
    let rail_token = css_block(TOKENS_CSS, ".quality-rail");
    assert!(
        rail_token.contains("display: flex") && !rail_token.contains("flex-direction: column"),
        "Quality Rail is a horizontal header instrument, not a left spine"
    );

    let opportunities = extract_surface(UI_BUNDLE, "opportunities");
    assert!(
        opportunities.contains("Opportunities"),
        "left column must be Opportunities at rest"
    );
    assert!(
        opportunities.contains("opportunity-roster")
            || opportunities.contains("id=\"opportunityRoster\""),
        "left column lists Opportunities"
    );
    let assessment = extract_surface(UI_BUNDLE, "assessment");
    assert!(
        assessment.contains("Project Assessment"),
        "center must land on Project Assessment"
    );
    assert!(
        !assessment.contains("class=\"modal\"")
            && !assessment.contains("data-overlay=\"sheet\"")
            && !assessment.contains("sheet-overlay"),
        "Project Assessment is not a modal"
    );
    assert!(
        !UI_BUNDLE.contains("id=\"reportModal\""),
        "Project Assessment must not be trapped in a modal"
    );

    let opp_pos = UI_BUNDLE
        .find("data-surface=\"opportunities\"")
        .expect("opportunities surface missing");
    let assess_pos = UI_BUNDLE
        .find("data-surface=\"assessment\"")
        .expect("assessment surface missing");
    let shell_pos = UI_BUNDLE
        .find("id=\"codeView\"")
        .expect("Shell chrome missing");
    assert!(
        shell_pos < opp_pos && opp_pos < assess_pos,
        "left is Opportunities at rest; center lands on Project Assessment"
    );
    let layout_left = attr_values(&opportunities, "data-shell-region");
    assert!(
        layout_left.iter().any(|v| v == "left"),
        "Opportunities occupy the left Shell region at rest"
    );
    assert!(
        UI_BUNDLE[assess_pos.saturating_sub(240)..assess_pos + 120]
            .contains("data-shell-region=\"center\"")
            || attr_values(&assessment, "data-shell-region")
                .iter()
                .any(|v| v == "center"),
        "Project Assessment occupies the center Shell region"
    );

    assert!(
        UI_BUNDLE.contains("data-returns=\"picker\"")
            && UI_BUNDLE.contains("function showProjectPicker"),
        "a control returns to the Project Picker without quitting"
    );
    let back = js_function_body(UI_BUNDLE, "showProjectPicker");
    assert!(
        back.contains("picker") && !back.contains("window.close"),
        "returning to the Project Picker must not quit"
    );

    let activity = extract_surface(UI_BUNDLE, "activity");
    assert!(
        activity.contains("activity-strip"),
        "Activity strip presence matches #54"
    );
    let activity_pos = UI_BUNDLE
        .find("data-surface=\"activity\"")
        .expect("activity surface missing");
    assert!(
        activity_pos > assess_pos,
        "Activity is a Shell sibling so navigation cannot unmount it"
    );
    assert!(
        !assessment.contains("data-surface=\"activity\"")
            && !opportunities.contains("data-surface=\"activity\""),
        "Activity stays mounted in the Shell strip, not inside Assessment or Opportunities"
    );
}

#[test]
fn excavate_mode_swaps_left_to_tree() {
    assert!(
        UI_BUNDLE.contains("data-enters=\"excavate\"")
            && UI_BUNDLE.contains("aria-label=\"Direct Excavate\""),
        "Direct Excavate must be available from the Shell"
    );
    assert!(
        UI_BUNDLE.contains("function enterExcavateMode"),
        "Direct Excavate must enter Excavate mode from the Shell"
    );
    let enter = js_function_body(UI_BUNDLE, "enterExcavateMode");
    assert!(
        enter.contains("excavate")
            && (enter.contains("dataset.mode") || enter.contains("data-mode")),
        "entering Excavate must set Shell mode to excavate, got {enter}"
    );

    let tree = extract_surface(UI_BUNDLE, "tree");
    assert!(
        tree.contains("file-tree") && tree.contains("data-tree=\"project\""),
        "Excavate mode left column must be the hierarchical Project file tree"
    );
    assert!(
        attr_values(&tree, "data-shell-region")
            .iter()
            .any(|value| value == "left"),
        "the file tree occupies the left Shell region in Excavate mode"
    );
    let opportunities = extract_surface(UI_BUNDLE, "opportunities");
    assert!(
        attr_values(&opportunities, "data-shell-region")
            .iter()
            .any(|value| value == "left"),
        "Opportunities still own the left region at rest"
    );
    assert!(
        css_block(
            UI_BUNDLE,
            "body[data-mode=excavate] [data-surface=opportunities]"
        )
        .contains("display: none"),
        "Excavate must swap Opportunities off the left column"
    );
    assert!(
        css_block(UI_BUNDLE, "body[data-mode=excavate] [data-surface=tree]")
            .contains("display: flex"),
        "Excavate must show the file tree in the left column"
    );

    let shell_css = css_block(UI_BUNDLE, ".shell-layout");
    assert!(
        shell_css.contains("grid-template-columns: 280px minmax(0, 1fr)"),
        "Excavate swaps the left column; it must not insert a fourth pane"
    );
    assert!(
        !UI_BUNDLE.contains("class=\"excavate-surfaces\""),
        "tree and code must occupy the existing Shell columns, not a new excavate pane"
    );

    let code = extract_surface(UI_BUNDLE, "code");
    assert!(
        code.contains("code-content") || code.contains("id=\"codeContainer\""),
        "center must become code while excavating"
    );
    assert!(
        attr_values(&code, "data-shell-region")
            .iter()
            .any(|value| value == "center"),
        "code occupies the center Shell region while excavating"
    );
    assert!(
        css_block(
            UI_BUNDLE,
            "body[data-mode=excavate] [data-surface=assessment]"
        )
        .contains("display: none"),
        "Project Assessment yields the center while excavating"
    );
    assert!(
        css_block(UI_BUNDLE, "body[data-mode=excavate] [data-surface=code]")
            .contains("display: flex"),
        "center is code while excavating"
    );

    let rail = extract_surface(UI_BUNDLE, "quality-rail");
    let activity = extract_surface(UI_BUNDLE, "activity");
    assert!(
        rail.contains("quality-rail") && activity.contains("activity-strip"),
        "Quality Rail and Activity remain while excavating"
    );
    assert!(
        !tree.contains("data-surface=\"quality-rail\"")
            && !tree.contains("data-surface=\"activity\"")
            && !code.contains("data-surface=\"quality-rail\"")
            && !code.contains("data-surface=\"activity\""),
        "Quality Rail and Activity stay mounted outside the swapped columns"
    );
    assert!(
        !UI_BUNDLE.contains("body[data-mode=\"excavate\"] [data-surface=\"quality-rail\"]")
            && !UI_BUNDLE.contains("body[data-mode=\"excavate\"] [data-surface=\"activity\"]")
            && !UI_BUNDLE.contains("body[data-mode=\"excavate\"] .quality-rail")
            && !UI_BUNDLE.contains("body[data-mode=\"excavate\"] .activity-strip"),
        "Excavate mode must not hide the Quality Rail or Activity"
    );

    assert!(
        UI_BUNDLE.contains("data-leaves=\"excavate\"")
            && UI_BUNDLE.contains("function leaveExcavateMode"),
        "leaving Excavate must be available on the Shell"
    );
    let leave = js_function_body(UI_BUNDLE, "leaveExcavateMode");
    assert!(
        leave.contains("shell") && (leave.contains("dataset.mode") || leave.contains("data-mode")),
        "leaving Excavate restores the Shell so Opportunities return on the left, got {leave}"
    );
    assert!(
        css_block(UI_BUNDLE, "[data-surface=tree]").contains("display: none"),
        "at rest the tree is not beside Opportunities"
    );
}

#[test]
fn field_notes_are_left_filter() {
    let opportunities = extract_surface(UI_BUNDLE, "opportunities");
    assert!(
        opportunities.contains("Field Notes"),
        "Field Notes must appear as a left-column filter or section"
    );
    assert!(
        attr_values(&opportunities, "data-shell-region")
            .iter()
            .any(|value| value == "left"),
        "Field Notes occupy the left Shell region with Opportunities"
    );
    assert!(
        opportunities.contains("data-left-filter=\"field-notes\"")
            && opportunities.contains("data-left-list=\"field-notes\""),
        "Field Notes are a left-column filter, not another room"
    );
    assert!(
        opportunities.contains("id=\"fieldNoteRoster\"")
            || opportunities.contains("field-note-roster"),
        "left column lists Field Notes"
    );
    assert!(
        !opportunities.contains("view-container"),
        "Field Notes are not a view-container room"
    );
    assert!(
        !contains_tab_pattern(&opportunities),
        "Field Notes filter is a section, not a tab strip"
    );

    let settings = extract_surface(UI_BUNDLE, "settings");
    assert!(
        !settings.contains("Field Notes")
            && !settings.contains("field-notes")
            && !settings.contains("data-settings-section=\"notes\""),
        "Field Notes must not be a Settings destination"
    );
    let settings_ids = attr_values(&settings, "data-settings-section");
    assert_eq!(
        settings_ids, SETTINGS_SECTIONS,
        "Settings keeps the five product sections; Field Notes are not among them"
    );

    assert!(
        UI_BUNDLE.contains("function selectFieldNote")
            || UI_BUNDLE.contains("async function selectFieldNote"),
        "opening a Field Note must be a left-column action"
    );
    let open = js_function_body(UI_BUNDLE, "selectFieldNote");
    assert!(
        open.contains("renderFinding") || open.contains("setStrataBody"),
        "opening a Field Note uses the same Strata as an Opportunity, got {open}"
    );
    assert!(
        !open.contains("openSettingsModal") && !open.contains("settingsSheet"),
        "opening a Field Note must not go to Settings"
    );
    assert!(
        !open.contains("enterExcavateMode"),
        "opening a Field Note drills Strata; it must not swap the Shell into Excavate"
    );
    let strata_count = UI_BUNDLE.matches("data-surface=\"strata\"").count();
    assert_eq!(
        strata_count, 1,
        "Field Notes drill the same Strata surface as Opportunities"
    );

    assert!(
        UI_BUNDLE.contains("list_field_notes"),
        "Field Notes roster reads the engine store"
    );

    assert!(
        css_block(UI_BUNDLE, "[data-left-list=field-notes]").contains("display: none"),
        "Field Notes list is a filter of the left column, hidden until selected"
    );
    assert!(
        css_block(
            UI_BUNDLE,
            "[data-surface=opportunities][data-left-filter=field-notes] [data-left-list=field-notes]"
        )
        .contains("display: flex"),
        "selecting the Field Notes filter must show the Field Notes list"
    );
    assert!(
        css_block(
            UI_BUNDLE,
            "[data-surface=opportunities][data-left-filter=field-notes] [data-left-list=opportunities]"
        )
        .contains("display: none"),
        "the Field Notes filter swaps the Opportunities list, not the Shell layout"
    );

    assert!(
        css_block(
            UI_BUNDLE,
            "body[data-mode=excavate] [data-surface=opportunities]"
        )
        .contains("display: none"),
        "Excavate still swaps Field Notes off the left with Opportunities"
    );
    let shell_css = css_block(UI_BUNDLE, ".shell-layout");
    assert!(
        shell_css.contains("grid-template-columns: 280px minmax(0, 1fr)"),
        "Field Notes stay in the left column; they must not insert a fourth pane"
    );
}

fn picker_mentions_site(markup: &str) -> bool {
    markup
        .split(|c: char| !c.is_ascii_alphabetic())
        .any(|word| word.eq_ignore_ascii_case("site") || word.eq_ignore_ascii_case("sites"))
}

fn names_next_verb(markup: &str) -> bool {
    let lower = markup.to_ascii_lowercase();
    ["open", "enter", "choose", "add", "assess", "try"]
        .iter()
        .any(|verb| lower.contains(verb))
}

fn extract_marked_element(html: &str, attr: &str, value: &str) -> String {
    let mark = format!("{attr}=\"{value}\"");
    let start = html.find(&mark).unwrap_or_else(|| panic!("missing {mark}"));
    let head = html[..start].rfind('<').unwrap_or(start);
    let tag_rest = &html[head + 1..];
    let tag_end = tag_rest
        .find(|c: char| c.is_whitespace() || c == '>')
        .unwrap_or(tag_rest.len());
    let tag = &tag_rest[..tag_end];
    let close = format!("</{tag}>");
    let after = &html[start..];
    let end_rel = after
        .find(&close)
        .unwrap_or_else(|| panic!("{attr}={value} is not a closed {tag}"));
    html[head..start + end_rel + close.len()].to_string()
}

fn extract_strata_surface(html: &str) -> String {
    const MARK: &str = "data-surface=\"strata\"";
    let start = html
        .find(MARK)
        .unwrap_or_else(|| panic!("missing {MARK} on the Opportunity detail surface"));
    let head = html[..start].rfind('<').unwrap_or(start);
    let after = &html[start..];
    let end_rel = after
        .find("</article>")
        .unwrap_or_else(|| panic!("strata surface is not a closed article"));
    html[head..start + end_rel + "</article>".len()].to_string()
}

fn strata_uses_column_css(html: &str) -> bool {
    css_block(html, ".strata").contains("flex-direction: column")
        && css_block(html, ".strata-section").contains("display: block")
}

fn css_block(html: &str, selector: &str) -> String {
    let needle = format!("{selector} {{");
    let start = match html.find(&needle) {
        Some(idx) => idx,
        None => return String::new(),
    };
    let rest = &html[start..];
    let end = rest.find('}').unwrap_or(rest.len());
    rest[..end].to_string()
}

fn contains_tab_pattern(markup: &str) -> bool {
    let lower = markup.to_ascii_lowercase();
    lower.contains("role=\"tablist\"")
        || lower.contains("role=\"tab\"")
        || lower.contains("filter-tab")
        || lower.contains("nav-tabs")
}

fn attr_values(html: &str, attr: &str) -> Vec<String> {
    let needle = format!("{attr}=\"");
    let mut values = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find(&needle) {
        let after = &rest[start + needle.len()..];
        match after.find('"') {
            Some(end) => {
                values.push(after[..end].to_string());
                rest = &after[end..];
            }
            None => break,
        }
    }
    values
}

fn heading_labels(html: &str) -> Vec<String> {
    let mut labels = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<h2") {
        let after = &rest[start..];
        match after.find('>') {
            Some(gt) => {
                let inner = &after[gt + 1..];
                match inner.find("</h2>") {
                    Some(end) => {
                        labels.push(inner[..end].trim().to_string());
                        rest = &inner[end + 5..];
                    }
                    None => break,
                }
            }
            None => break,
        }
    }
    labels
}

fn extract_surface(html: &str, surface: &str) -> String {
    let mark = format!("data-surface=\"{surface}\"");
    let start = html.find(&mark).unwrap_or_else(|| panic!("missing {mark}"));
    let head = html[..start].rfind('<').unwrap_or(start);
    let tag_rest = &html[head + 1..];
    let tag_end = tag_rest
        .find(|c: char| c.is_whitespace() || c == '>')
        .unwrap_or(tag_rest.len());
    let tag = &tag_rest[..tag_end];
    let close = format!("</{tag}>");
    let after = &html[start..];
    let end_rel = after
        .find(&close)
        .unwrap_or_else(|| panic!("{surface} surface is not a closed {tag}"));
    html[head..start + end_rel + close.len()].to_string()
}

fn sheet_uses_overlay_css(html: &str) -> bool {
    let overlay = css_block(html, ".sheet-overlay");
    overlay.contains("position: fixed") && overlay.contains("display: none")
}

fn js_function_body(html: &str, name: &str) -> String {
    let async_fn = format!("async function {name}(");
    let sync_fn = format!("function {name}(");
    let start = html
        .find(&async_fn)
        .or_else(|| html.find(&sync_fn))
        .unwrap_or_else(|| panic!("missing function {name}"));
    let rest = &html[start..];
    let mut cut = rest.len();
    for (idx, _) in rest.match_indices('\n') {
        if idx == 0 {
            continue;
        }
        let line = rest[idx + 1..].split('\n').next().unwrap_or("");
        if line.starts_with("    function ") || line.starts_with("    async function ") {
            cut = idx;
            break;
        }
        if line.starts_with("  </script>") || line.starts_with("</script>") {
            cut = idx;
            break;
        }
    }
    rest[..cut].to_string()
}
