use std::path::Path;

const UI_BUNDLE: &str = include_str!("../ui/index.html");

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
