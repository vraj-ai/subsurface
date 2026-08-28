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

    for kind in [
        "assessment",
        "preparation",
        "verification",
        "publication",
    ] {
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
