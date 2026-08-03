use super::{mobile_web_companion_command, mobile_web_port_for_command};
use crate::CliContextPaths;
use ajax_core::config::RuntimePathRequest;
use std::ffi::OsStr;

#[test]
fn dev_mobile_web_companion_uses_dev_port() {
    assert_eq!(mobile_web_port_for_command("dev"), 8788);
}

#[test]
fn mobile_web_companion_preserves_full_dev_runtime_context() {
    let paths = CliContextPaths::from_runtime_paths(
        RuntimePathRequest::new("/Users/matt")
            .with_cli_profile("dev")
            .resolve(),
    );
    let command = mobile_web_companion_command(
        std::path::Path::new("/tmp/ajax-cli"),
        mobile_web_port_for_command("dev"),
        Some(&paths),
    );
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let envs = command.get_envs().collect::<Vec<_>>();

    assert_eq!(args, ["web", "--host", "0.0.0.0", "--port", "8788"]);
    assert!(envs.contains(&(
        OsStr::new("AJAX_PROFILE"),
        Some(OsStr::new(paths.runtime_paths.profile.as_str()))
    )));
    assert!(envs.contains(&(
        OsStr::new("AJAX_CONFIG"),
        Some(paths.config_file.as_os_str())
    )));
    assert!(envs.contains(&(OsStr::new("AJAX_STATE"), Some(paths.state_file.as_os_str()))));
    assert!(envs.iter().any(|(name, value)| {
        *name == OsStr::new("AJAX_WORKTREE_ROOT")
            && value.is_some_and(|value| value == "/Users/matt/.ajax-dev/worktrees")
    }));
}
