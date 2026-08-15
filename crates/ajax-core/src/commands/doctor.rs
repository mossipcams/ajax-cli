use super::CommandContext;
use crate::{
    adapters::{acp_adapter_packages, DoctorEnvironment, REQUIRED_DOCTOR_TOOLS},
    output::{DoctorCheck, DoctorResponse},
    registry::Registry,
};
use std::collections::BTreeSet;

fn agent_label(client: crate::models::AgentClient) -> &'static str {
    match client {
        crate::models::AgentClient::Claude => "claude",
        crate::models::AgentClient::Codex => "codex",
        crate::models::AgentClient::Cursor => "cursor",
        crate::models::AgentClient::Pi => "pi",
        crate::models::AgentClient::Other => "other",
    }
}

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
    // Browser sessions drive Codex, Claude, and Pi through their Agent Client
    // Protocol adapters. They are separate installs, so name the missing one
    // rather than letting a session fail with an empty model list.
    checks.extend(
        acp_adapter_packages()
            .into_iter()
            .map(|(client, program, package)| {
                let ok = environment.has_tool(program);
                DoctorCheck {
                    name: format!("acp:{}", agent_label(client)),
                    ok,
                    message: if ok {
                        format!("{program} available")
                    } else {
                        format!("{program} not found on PATH — npm install -g {package}")
                    },
                }
            }),
    );
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
