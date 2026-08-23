#[test]
fn project_commands_are_registered() {
    let source = include_str!("../src/main.rs");

    for command in ["open_project", "assess_project", "list_project_activities"] {
        assert!(
            source.contains(&format!("fn {command}(")),
            "missing {command} command"
        );
        assert!(
            source.contains(&format!("            {command},")),
            "{command} is not registered with Tauri"
        );
    }

    assert!(source.contains("fn open_site("), "legacy command removed during expand");
    assert!(source.contains("fn generate_report("), "legacy command removed during expand");
    assert!(source.contains("record_activity(&project.root_path, \"project_open\""));
    assert!(source.contains("record_activity(&project.root_path, \"assessment\""));
    assert!(source.contains("ActivityStatus::Failed"));
}
