#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    const SLICES: [&str; 4] = ["cockpit", "install", "operate", "terminal"];
    const ADAPTERS: [&str; 3] = ["assets", "http", "tls"];

    const FORBIDDEN_RUNTIME_DEPENDENCIES: [&str; 2] = ["ajax-web::runtime", "crate::runtime"];

    #[test]
    fn each_web_adapter_does_not_depend_on_slices_or_runtime() {
        for adapter in ADAPTERS {
            let forbidden_slices = forbidden_paths_for_slices(&SLICES);
            let forbidden_runtime = forbidden_runtime_dependencies();
            let forbidden = forbidden_slices
                .iter()
                .chain(forbidden_runtime.iter())
                .cloned()
                .collect::<Vec<_>>();
            let module = format!("ajax-web::adapters::{adapter}");

            assert_module_does_not_depend_on(&module, &forbidden, "adapter", adapter);
        }
    }

    #[test]
    fn actions_module_does_not_depend_on_sibling_slices_or_runtime() {
        let forbidden_slices = forbidden_paths_for_slices(&SLICES);
        let forbidden_runtime = forbidden_runtime_dependencies();
        let forbidden = forbidden_slices
            .iter()
            .chain(forbidden_runtime.iter())
            .cloned()
            .collect::<Vec<_>>();

        assert_module_does_not_depend_on(
            "ajax-web::slices::actions",
            &forbidden,
            "module",
            "actions",
        );
    }

    #[test]
    fn each_web_slice_is_isolated_from_sibling_slices_and_runtime() {
        for slice in SLICES {
            let forbidden_siblings = forbidden_paths_for_sibling_slices(slice);
            let forbidden_runtime = forbidden_runtime_dependencies();
            let forbidden = forbidden_siblings
                .iter()
                .chain(forbidden_runtime.iter())
                .cloned()
                .collect::<Vec<_>>();
            let module = format!("ajax-web::slices::{slice}");

            assert_module_does_not_depend_on(&module, &forbidden, "slice", slice);
        }
    }

    #[test]
    fn architecture_rule_rejects_cross_slice_dependency() {
        assert!(
            source_mentions_dependency(
                "use crate::slices::operate::OperateRequest;",
                &forbidden_paths_for_sibling_slices("cockpit")
            ),
            "web slices must be independent of sibling slices"
        );
    }

    #[test]
    fn architecture_rule_rejects_adapter_importing_specific_slice() {
        assert!(
            source_mentions_dependency(
                "use crate::slices::install::browser_shell;",
                &forbidden_paths_for_slices(&["install"])
            ),
            "web adapter mechanisms must not import any specific slice"
        );
    }

    fn forbidden_paths_for_slices(slices: &[&str]) -> Vec<String> {
        slices
            .iter()
            .flat_map(|slice| {
                [
                    format!("ajax-web::slices::{slice}"),
                    format!("crate::slices::{slice}"),
                ]
            })
            .collect()
    }

    fn forbidden_paths_for_sibling_slices(slice: &str) -> Vec<String> {
        let siblings = SLICES
            .iter()
            .copied()
            .filter(|sibling| *sibling != slice)
            .collect::<Vec<_>>();
        forbidden_paths_for_slices(&siblings)
    }

    fn forbidden_runtime_dependencies() -> Vec<String> {
        FORBIDDEN_RUNTIME_DEPENDENCIES
            .iter()
            .map(|dependency| (*dependency).to_string())
            .collect()
    }

    fn assert_module_does_not_depend_on(
        module: &str,
        forbidden: &[String],
        kind: &str,
        name: &str,
    ) {
        let violations = module_sources(module)
            .into_iter()
            .filter_map(|path| {
                let source = std::fs::read_to_string(&path).unwrap();
                source_mentions_dependency(&source, forbidden).then_some(path)
            })
            .collect::<Vec<_>>();

        assert!(
            violations.is_empty(),
            "architecture violations in {kind} `{name}`: {violations:#?}"
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
