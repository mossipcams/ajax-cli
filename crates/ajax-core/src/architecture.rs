#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    const OPERATION_SLICES: [&str; 7] = [
        "start",
        "resume",
        "review",
        "repair",
        "ship",
        "drop_task",
        "sweep_cleanup",
    ];

    // sweep_cleanup composes drop_task teardown (tidy sweeps what drop leaves); kernel and
    // operator_dispatch are shared/composition plumbing and are not operator slices.
    const ALLOWED_SLICE_DEPENDENCIES: [(&str, &str); 1] = [("sweep_cleanup", "drop_task")];

    const KERNEL_MODULES: [&str; 11] = [
        "models",
        "lifecycle",
        "live",
        "live_application",
        "agent_status",
        "ui_state",
        "attention",
        "policy",
        "output",
        "ghost_task",
        "validity",
    ];

    #[test]
    fn task_operations_submodules_are_file_backed() {
        let source = std::fs::read_to_string("src/task_operations.rs").unwrap();
        for name in [
            "kernel",
            "operator_dispatch",
            "start",
            "resume",
            "review",
            "repair",
            "ship",
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
        assert!(
            !source.contains("pub mod task_command;"),
            "task_command is no longer an operator slice; use resume/review/repair/ship + operator_dispatch"
        );
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

    #[test]
    fn each_task_operation_slice_is_isolated_from_sibling_slices() {
        for slice in OPERATION_SLICES {
            let forbidden = OPERATION_SLICES
                .iter()
                .copied()
                .filter(|sibling| *sibling != slice)
                .filter(|sibling| !ALLOWED_SLICE_DEPENDENCIES.contains(&(slice, *sibling)))
                .flat_map(|sibling| {
                    [
                        format!("crate::task_operations::{sibling}"),
                        format!("task_operations::{sibling}::"),
                    ]
                })
                .collect::<Vec<_>>();
            if forbidden.is_empty() {
                continue;
            }

            assert_module_does_not_depend_on(
                &format!("ajax-core::task_operations::{slice}"),
                &forbidden,
                "operation slice",
                slice,
            );
        }
    }

    #[test]
    fn each_task_operation_slice_declares_its_operation_entry_points() {
        for slice in OPERATION_SLICES {
            let source = slice_sources(slice).join("\n");

            assert!(
                source.contains("pub fn execute_"),
                "task operation slice `{slice}` should declare an execute_ entry point"
            );
            if slice != "sweep_cleanup" {
                assert!(
                    source.contains("pub fn plan_"),
                    "task operation slice `{slice}` should declare a plan_ entry point"
                );
            }
        }
    }

    #[test]
    fn shared_kernel_does_not_depend_on_commands_or_task_operations() {
        let forbidden = [
            "crate::task_operations".to_string(),
            "crate::commands".to_string(),
            "task_operations::".to_string(),
        ];
        for module in KERNEL_MODULES {
            let path = PathBuf::from("src").join(format!("{module}.rs"));
            let dir = PathBuf::from("src").join(module);
            if !path.exists() && !dir.exists() {
                continue;
            }
            assert_module_does_not_depend_on(
                &format!("ajax-core::{module}"),
                &forbidden,
                "shared kernel module",
                module,
            );
        }
    }

    #[test]
    fn adapters_do_not_depend_on_task_operations() {
        let forbidden = [
            "crate::task_operations".to_string(),
            "task_operations::".to_string(),
        ];
        assert_module_does_not_depend_on(
            "ajax-core::adapters",
            &forbidden,
            "mechanism module",
            "adapters",
        );
    }

    #[test]
    fn models_module_does_not_homonym_top_level_events() {
        let source = std::fs::read_to_string("src/models/mod.rs").unwrap();
        assert!(
            !source.contains("pub mod events;"),
            "models must not declare pub mod events; that homonym creates an import cycle with crate::events"
        );
        assert!(
            !PathBuf::from("src/models/events.rs").exists(),
            "models/events.rs must stay renamed (step_receipts.rs) to avoid homonym with src/events.rs"
        );
    }

    #[test]
    fn commands_do_not_import_operator_slices() {
        let forbidden = OPERATION_SLICES
            .iter()
            .flat_map(|slice| {
                [
                    format!("crate::task_operations::{slice}"),
                    format!("task_operations::{slice}::"),
                    format!("task_operations::{slice};"),
                ]
            })
            .collect::<Vec<_>>();
        assert_module_does_not_depend_on(
            "ajax-core::commands",
            &forbidden,
            "plan helper module",
            "commands",
        );
    }

    fn slice_sources(slice: &str) -> Vec<String> {
        module_sources(&format!("ajax-core::task_operations::{slice}"))
            .into_iter()
            .map(|path| std::fs::read_to_string(path).unwrap())
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
        let dir = PathBuf::from("src").join(&relative);
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
        // Only expand nested parents like `task_operations::{sibling}`. Bare
        // `crate::{ ... }` matches too many unrelated imports.
        if !parent.contains("::") {
            return false;
        }
        source.contains(&format!("{parent}::{{")) && source.contains(child)
    }
}
