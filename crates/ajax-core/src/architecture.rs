#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    const SLICES: [&str; 1] = ["pane"];

    const SUBSTRATE_MECHANISMS: [&str; 4] = ["adapters", "registry", "analysis", "runtime"];

    #[test]
    fn each_substrate_mechanism_does_not_depend_on_any_slice() {
        for mechanism in SUBSTRATE_MECHANISMS {
            let forbidden = forbidden_paths_for_slices(&SLICES);
            let module = format!("ajax-core::{mechanism}");

            assert_module_does_not_depend_on(&module, &forbidden, "mechanism", mechanism);
        }
    }

    #[test]
    fn each_slice_is_isolated_from_sibling_slices() {
        for slice in SLICES {
            let forbidden = forbidden_paths_for_sibling_slices(slice);
            if forbidden.is_empty() {
                continue;
            }
            let module = format!("ajax-core::slices::{slice}");

            assert_module_does_not_depend_on(&module, &forbidden, "slice", slice);
        }
    }

    #[test]
    fn architecture_rule_rejects_use_crate_slices_dependency() {
        assert!(
            source_mentions_dependency(
                "use crate::slices::pane;",
                &forbidden_paths_for_slices(&["pane"])
            ),
            "mechanism modules must not be allowed to import specific slice modules"
        );
    }

    #[test]
    fn architecture_rule_rejects_direct_crate_slices_dependency() {
        assert!(
            source_mentions_dependency(
                "fn example() { crate::slices::pane::capture_prompt(); }",
                &forbidden_paths_for_slices(&["pane"])
            ),
            "mechanism modules must not be allowed to call specific slice modules directly"
        );
    }

    #[test]
    fn task_operations_submodules_are_file_backed() {
        let source = std::fs::read_to_string("src/task_operations.rs").unwrap();
        for name in [
            "kernel",
            "start",
            "task_command",
            "drop_task",
            "sweep_cleanup",
        ] {
            assert!(
                source.contains(&format!("pub mod {name};")),
                "task_operations.rs should declare {name} as a file-backed submodule"
            );
            assert!(
                !source.contains(&format!("pub mod {name} {{")),
                "task_operations.rs should not contain an inline {name} module body"
            );
        }
    }

    #[test]
    fn commands_module_does_not_own_external_command_execution_loop() {
        let source = std::fs::read_to_string("src/commands.rs").unwrap();

        assert!(
            !source.contains("for command in &plan.commands {"),
            "commands.rs should not own the external command execution loop"
        );
        if source.contains("pub fn execute_plan(") {
            assert!(
                source.contains("task_operations::kernel::execute_external_plan"),
                "execute_plan should only remain as a thin compatibility wrapper"
            );
        }
    }

    #[test]
    fn check_and_merge_do_not_mutate_tasks_through_raw_registry_access() {
        for file in ["src/commands/check.rs", "src/commands/merge.rs"] {
            let source = std::fs::read_to_string(file).unwrap();
            assert!(
                !source.contains(".get_task_mut("),
                "{file} should mutate task lifecycle through typed helpers instead of raw registry access"
            );
        }
    }

    fn forbidden_paths_for_slices(slices: &[&str]) -> Vec<String> {
        slices
            .iter()
            .flat_map(|slice| {
                [
                    format!("ajax-core::slices::{slice}"),
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
