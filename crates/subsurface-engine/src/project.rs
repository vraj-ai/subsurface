pub use crate::site::{
    Site as Project, SiteError as ProjectError, SiteIndexReport as ProjectIndexReport,
};

#[cfg(test)]
mod tests {
    use super::Project;
    use crate::fixture::GitFixture;

    #[test]
    fn opens_project_with_the_existing_cheap_index() {
        let mut fixture = GitFixture::new();
        fixture.commit("initial", &[("src/lib.rs", "// code\n")]);

        let project = Project::open(fixture.path()).expect("open project");

        assert_eq!(project.total_commits_estimate, 1);
        assert!(project.tracked_files.contains(&"src/lib.rs".to_string()));
        assert!(!project.index_report.expensive_walks_indexed);
    }
}
