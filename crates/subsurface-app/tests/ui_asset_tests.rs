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
