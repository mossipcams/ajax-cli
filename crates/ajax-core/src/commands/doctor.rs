use super::CommandContext;
use crate::{
    adapters::{DoctorEnvironment, REQUIRED_DOCTOR_TOOLS},
    output::{DoctorCheck, DoctorResponse},
    registry::Registry,
};
use std::collections::BTreeSet;

pub fn doctor<R: Registry>(context: &CommandContext<R>) -> DoctorResponse {
    doctor_with_environment(context, &DoctorEnvironment::from_path())
}

pub fn doctor_with_environment<R: Registry>(
    context: &CommandContext<R>,
    environment: &DoctorEnvironment,
) -> DoctorResponse {
    let mut checks = vec![
        DoctorCheck {
            name: "config".to_string(),
            ok: true,
            message: format!("{} repo(s) configured", context.config.repos.len()),
        },
        DoctorCheck {
            name: "registry".to_string(),
            ok: true,
            message: format!("{} task(s) tracked", context.registry.list_tasks().len()),
        },
    ];

    checks.extend(REQUIRED_DOCTOR_TOOLS.iter().map(|tool| {
        let ok = environment.has_tool(tool);
        DoctorCheck {
            name: format!("tool:{tool}"),
            ok,
            message: if ok {
                "available".to_string()
            } else {
                "not found on PATH".to_string()
            },
        }
    }));
    checks.push(repo_name_check(context));
    for repo in &context.config.repos {
        let repo_path_exists = environment.path_exists(&repo.path);
        checks.push(DoctorCheck {
            name: format!("repo:{}:path", repo.name),
            ok: repo_path_exists,
            message: if repo_path_exists {
                format!("path exists: {}", repo.path.display())
            } else {
                format!("path missing: {}", repo.path.display())
            },
        });

        let has_test_command = context
            .config
            .test_commands
            .iter()
            .any(|test_command| test_command.repo == repo.name);
        checks.push(DoctorCheck {
            name: format!("repo:{}:test-command", repo.name),
            ok: has_test_command,
            message: if has_test_command {
                "test command configured".to_string()
            } else {
                "no test command configured".to_string()
            },
        });
    }

    DoctorResponse { checks }
}

fn repo_name_check<R: Registry>(context: &CommandContext<R>) -> DoctorCheck {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();

    for repo in &context.config.repos {
        if !seen.insert(repo.name.clone()) {
            duplicates.insert(repo.name.clone());
        }
    }

    if let Some(duplicate) = duplicates.into_iter().next() {
        DoctorCheck {
            name: "config:repo-names".to_string(),
            ok: false,
            message: format!("duplicate repo name: {duplicate}"),
        }
    } else {
        DoctorCheck {
            name: "config:repo-names".to_string(),
            ok: true,
            message: "repo names unique".to_string(),
        }
    }
}
