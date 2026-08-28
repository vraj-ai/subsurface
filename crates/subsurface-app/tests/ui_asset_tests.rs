use std::path::Path;

const UI_BUNDLE: &str = include_str!("../ui/index.html");

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
