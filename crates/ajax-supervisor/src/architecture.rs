#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    const FORBIDDEN_RUNTIME_DEPENDENCIES: [&str; 2] =
        ["ajax-supervisor::runtime", "crate::runtime"];

    #[test]
    fn supervisor_substrates_do_not_depend_on_runtime() {
        for module in [
            "agent",
            "event_log",
            "process_observer",
            "repo_observer",
            "renderer",
            "status",
        ] {
            assert_module_does_not_depend_on(
                &format!("ajax-supervisor::{module}"),
                &forbidden_runtime_dependencies(),
                module,
            );
        }
    }

    #[test]
    fn architecture_rule_rejects_observer_importing_runtime() {
        assert!(
            source_mentions_dependency(
                "use crate::runtime::spawn_monitor;",
                &forbidden_runtime_dependencies()
            ),
            "supervisor substrate observers must not depend on the runtime composer"
        );
    }

    fn forbidden_runtime_dependencies() -> Vec<String> {
        FORBIDDEN_RUNTIME_DEPENDENCIES
            .iter()
            .map(|dependency| (*dependency).to_string())
            .collect()
    }

    fn assert_module_does_not_depend_on(module: &str, forbidden: &[String], name: &str) {
        let violations = module_sources(module)
            .into_iter()
            .filter_map(|path| {
                let source = std::fs::read_to_string(&path).unwrap();
                source_mentions_dependency(&source, forbidden).then_some(path)
            })
            .collect::<Vec<_>>();

        assert!(
            violations.is_empty(),
            "architecture violations in `{name}`: {violations:#?}"
        );
    }

    fn module_sources(module: &str) -> Vec<PathBuf> {
        let relative = module.split("::").skip(1).collect::<Vec<_>>().join("/");
        let file = PathBuf::from("src").join(format!("{relative}.rs"));
        let dir = PathBuf::from("src").join(relative);
        let mut sources = Vec::new();
        if file.exists() {
            sources.push(file);
        }
        if dir.exists() {
            collect_rust_files(&dir, &mut sources);
        }
        sources
    }

    fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect_rust_files(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }

    fn source_mentions_dependency(source: &str, forbidden: &[String]) -> bool {
        forbidden
            .iter()
            .any(|dependency| source_mentions_path(source, dependency))
    }

    fn source_mentions_path(source: &str, dependency: &str) -> bool {
        if source.contains(dependency) {
            return true;
        }
        let Some((parent, child)) = dependency.rsplit_once("::") else {
            return false;
        };
        source.contains(&format!("{parent}::{{")) && source.contains(child)
    }
}
