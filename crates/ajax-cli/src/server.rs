use ajax_core::adapters::{CommandMode, CommandRunner, CommandSpec};
use clap::ArgMatches;
use std::path::{Path, PathBuf};

use crate::{CliError, RenderedCommand};

pub(crate) const LAUNCHD_LABEL: &str = "com.ajax.web";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LaunchdAction {
    Start,
    Stop,
    Restart,
    Status,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LaunchdConfig {
    ajax_executable: PathBuf,
    home_dir: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstallPlan {
    pub(crate) directory: PathBuf,
    pub(crate) plist_path: PathBuf,
    pub(crate) plist: String,
}

impl LaunchdConfig {
    pub(crate) fn new(ajax_executable: PathBuf, home_dir: PathBuf) -> Self {
        Self {
            ajax_executable,
            home_dir,
        }
    }

    pub(crate) fn plist_path(&self) -> PathBuf {
        self.launch_agents_dir()
            .join(format!("{LAUNCHD_LABEL}.plist"))
    }

    pub(crate) fn program_arguments(&self) -> Vec<String> {
        vec![
            self.ajax_executable.display().to_string(),
            "web".to_string(),
            "--host".to_string(),
            "0.0.0.0".to_string(),
            "--port".to_string(),
            "8787".to_string(),
        ]
    }

    pub(crate) fn render_plist(&self) -> String {
        let program_arguments = self
            .program_arguments()
            .into_iter()
            .map(|argument| format!("        <string>{}</string>", escape_plist_value(&argument)))
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
{}
    </array>
    <key>KeepAlive</key>
    <true/>
    <key>RunAtLoad</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{}</string>
    <key>StandardErrorPath</key>
    <string>{}</string>
</dict>
</plist>
"#,
            LAUNCHD_LABEL,
            program_arguments,
            escape_plist_path(&self.stdout_log_path()),
            escape_plist_path(&self.stderr_log_path()),
        )
    }

    fn launch_agents_dir(&self) -> PathBuf {
        self.home_dir.join("Library").join("LaunchAgents")
    }

    fn logs_dir(&self) -> PathBuf {
        self.home_dir.join("Library").join("Logs")
    }

    fn stdout_log_path(&self) -> PathBuf {
        self.logs_dir().join("ajax-web.out.log")
    }

    fn stderr_log_path(&self) -> PathBuf {
        self.logs_dir().join("ajax-web.err.log")
    }
}

pub(crate) fn plan_install(config: &LaunchdConfig) -> InstallPlan {
    InstallPlan {
        directory: config.launch_agents_dir(),
        plist_path: config.plist_path(),
        plist: config.render_plist(),
    }
}

pub(crate) fn install_launchd_job(config: &LaunchdConfig) -> Result<String, CliError> {
    let plan = plan_install(config);
    std::fs::create_dir_all(&plan.directory).map_err(|error| {
        CliError::CommandFailed(format!(
            "could not create launchd directory {}: {error}",
            plan.directory.display()
        ))
    })?;
    std::fs::create_dir_all(config.logs_dir()).map_err(|error| {
        CliError::CommandFailed(format!(
            "could not create ajax log directory {}: {error}",
            config.logs_dir().display()
        ))
    })?;
    std::fs::write(&plan.plist_path, plan.plist).map_err(|error| {
        CliError::CommandFailed(format!(
            "could not write launchd plist {}: {error}",
            plan.plist_path.display()
        ))
    })?;

    Ok(format!(
        "installed ajax web server launchd job at {}\n",
        plan.plist_path.display()
    ))
}

pub(crate) fn run_launchd_action(
    action: LaunchdAction,
    config: &LaunchdConfig,
    domain: &str,
    runner: &mut impl CommandRunner,
) -> Result<String, CliError> {
    let commands = launchd_commands(action, config, domain);
    for command in &commands {
        runner
            .run(command)
            .map_err(|error| CliError::CommandFailed(format!("launchd command failed: {error}")))?;
    }

    Ok(match action {
        LaunchdAction::Start => "started ajax web server\n".to_string(),
        LaunchdAction::Stop => "stopped ajax web server\n".to_string(),
        LaunchdAction::Restart => "restarted ajax web server\n".to_string(),
        LaunchdAction::Status => "ajax web server status checked\n".to_string(),
    })
}

pub(crate) fn render_server_command_with_config(
    subcommand: &ArgMatches,
    config: &LaunchdConfig,
    domain: &str,
    runner: &mut impl CommandRunner,
) -> Result<RenderedCommand, CliError> {
    let output = match subcommand.subcommand() {
        Some(("install", _)) => install_launchd_job(config)?,
        Some(("start", _)) => run_launchd_action(LaunchdAction::Start, config, domain, runner)?,
        Some(("stop", _)) => run_launchd_action(LaunchdAction::Stop, config, domain, runner)?,
        Some(("restart", _)) => run_launchd_action(LaunchdAction::Restart, config, domain, runner)?,
        Some(("status", _)) => run_launchd_action(LaunchdAction::Status, config, domain, runner)?,
        _ => {
            return Err(CliError::CommandFailed(
                "server subcommand is required".to_string(),
            ));
        }
    };

    Ok(RenderedCommand {
        output,
        state_changed: false,
    })
}

pub(crate) fn render_server_command(
    subcommand: &ArgMatches,
    runner: &mut impl CommandRunner,
) -> Result<RenderedCommand, CliError> {
    let config = default_launchd_config()?;
    let domain = current_launchd_domain()?;
    render_server_command_with_config(subcommand, &config, &domain, runner)
}

fn default_launchd_config() -> Result<LaunchdConfig, CliError> {
    let ajax_executable = std::env::current_exe().map_err(|error| {
        CliError::CommandFailed(format!("could not locate ajax executable: {error}"))
    })?;
    let home_dir = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
        CliError::CommandFailed("HOME is required for launchd plist path".to_string())
    })?;

    Ok(LaunchdConfig::new(ajax_executable, home_dir))
}

fn current_launchd_domain() -> Result<String, CliError> {
    let output = std::process::Command::new("id")
        .arg("-u")
        .output()
        .map_err(|error| {
            CliError::CommandFailed(format!("could not determine user id: {error}"))
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(CliError::CommandFailed(format!(
            "could not determine user id: {stderr}"
        )));
    }

    let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if uid.is_empty() {
        return Err(CliError::CommandFailed(
            "could not determine user id: id -u returned no output".to_string(),
        ));
    }

    Ok(format!("gui/{uid}"))
}

fn launchd_commands(
    action: LaunchdAction,
    config: &LaunchdConfig,
    domain: &str,
) -> Vec<CommandSpec> {
    match action {
        LaunchdAction::Start => vec![
            command_spec(
                "launchctl",
                vec![
                    "bootstrap".to_string(),
                    domain.to_string(),
                    config.plist_path().display().to_string(),
                ],
            ),
            command_spec(
                "launchctl",
                vec![
                    "kickstart".to_string(),
                    "-k".to_string(),
                    service_target(domain),
                ],
            ),
        ],
        LaunchdAction::Stop => vec![command_spec(
            "launchctl",
            vec!["bootout".to_string(), service_target(domain)],
        )],
        LaunchdAction::Restart => vec![command_spec(
            "launchctl",
            vec![
                "kickstart".to_string(),
                "-k".to_string(),
                service_target(domain),
            ],
        )],
        LaunchdAction::Status => vec![command_spec(
            "launchctl",
            vec!["print".to_string(), service_target(domain)],
        )],
    }
}

fn service_target(domain: &str) -> String {
    format!("{domain}/{LAUNCHD_LABEL}")
}

fn command_spec(program: &str, args: Vec<String>) -> CommandSpec {
    CommandSpec {
        program: program.to_string(),
        args,
        cwd: None,
        mode: CommandMode::Capture,
    }
}

fn escape_plist_path(path: &Path) -> String {
    escape_plist_value(&path.display().to_string())
}

fn escape_plist_value(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use ajax_core::adapters::{CommandSpec, RecordingCommandRunner};
    use std::path::PathBuf;

    use super::{
        plan_install, render_server_command_with_config, run_launchd_action, LaunchdAction,
        LaunchdConfig, LAUNCHD_LABEL,
    };

    #[test]
    fn launchd_config_uses_ajax_web_label_and_user_plist_path() {
        let config = LaunchdConfig::new(
            PathBuf::from("/usr/local/bin/ajax"),
            PathBuf::from("/Users/matt"),
        );

        assert_eq!(LAUNCHD_LABEL, "com.ajax.web");
        assert_eq!(
            config.plist_path(),
            PathBuf::from("/Users/matt/Library/LaunchAgents/com.ajax.web.plist")
        );
    }

    #[test]
    fn launchd_config_runs_mobile_web_command_with_keepalive() {
        let config = launchd_config();

        assert_eq!(
            config.program_arguments(),
            vec![
                "/usr/local/bin/ajax".to_string(),
                "web".to_string(),
                "--host".to_string(),
                "0.0.0.0".to_string(),
                "--port".to_string(),
                "8787".to_string(),
            ]
        );

        let plist = config.render_plist();

        assert!(plist.contains("<key>Label</key>"));
        assert!(plist.contains("<string>com.ajax.web</string>"));
        assert!(plist.contains("<key>ProgramArguments</key>"));
        assert!(plist.contains("<string>/usr/local/bin/ajax</string>"));
        assert!(plist.contains("<string>web</string>"));
        assert!(plist.contains("<string>--host</string>"));
        assert!(plist.contains("<string>0.0.0.0</string>"));
        assert!(plist.contains("<string>--port</string>"));
        assert!(plist.contains("<string>8787</string>"));
        assert!(plist.contains("<key>KeepAlive</key>"));
        assert!(plist.contains("<true/>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<key>StandardOutPath</key>"));
        assert!(plist.contains("/Users/matt/Library/Logs/ajax-web.out.log"));
        assert!(plist.contains("<key>StandardErrorPath</key>"));
        assert!(plist.contains("/Users/matt/Library/Logs/ajax-web.err.log"));
    }

    #[test]
    fn start_bootstraps_and_kickstarts_launchd_job() {
        let config = launchd_config();
        let mut runner = RecordingCommandRunner::default();

        let output =
            run_launchd_action(LaunchdAction::Start, &config, "gui/501", &mut runner).unwrap();

        assert_eq!(
            runner.commands(),
            &[
                command(
                    "launchctl",
                    &[
                        "bootstrap",
                        "gui/501",
                        "/Users/matt/Library/LaunchAgents/com.ajax.web.plist",
                    ],
                ),
                command("launchctl", &["kickstart", "-k", "gui/501/com.ajax.web"],),
            ]
        );
        assert!(output.contains("started ajax web server"));
    }

    #[test]
    fn stop_boots_out_launchd_job() {
        let config = launchd_config();
        let mut runner = RecordingCommandRunner::default();

        let output =
            run_launchd_action(LaunchdAction::Stop, &config, "gui/501", &mut runner).unwrap();

        assert_eq!(
            runner.commands(),
            &[command("launchctl", &["bootout", "gui/501/com.ajax.web"])]
        );
        assert!(output.contains("stopped ajax web server"));
    }

    #[test]
    fn restart_asks_launchd_to_restart_job() {
        let config = launchd_config();
        let mut runner = RecordingCommandRunner::default();

        let output =
            run_launchd_action(LaunchdAction::Restart, &config, "gui/501", &mut runner).unwrap();

        assert_eq!(
            runner.commands(),
            &[command(
                "launchctl",
                &["kickstart", "-k", "gui/501/com.ajax.web"],
            )]
        );
        assert!(output.contains("restarted ajax web server"));
    }

    #[test]
    fn status_prints_launchd_job() {
        let config = launchd_config();
        let mut runner = RecordingCommandRunner::default();

        let output =
            run_launchd_action(LaunchdAction::Status, &config, "gui/501", &mut runner).unwrap();

        assert_eq!(
            runner.commands(),
            &[command("launchctl", &["print", "gui/501/com.ajax.web"])]
        );
        assert!(output.contains("ajax web server status"));
    }

    #[test]
    fn install_plan_writes_launch_agents_plist() {
        let config = launchd_config();

        let plan = plan_install(&config);

        assert_eq!(
            plan.directory,
            PathBuf::from("/Users/matt/Library/LaunchAgents")
        );
        assert_eq!(
            plan.plist_path,
            PathBuf::from("/Users/matt/Library/LaunchAgents/com.ajax.web.plist")
        );
        assert!(plan.plist.contains("<key>Label</key>"));
        assert!(plan.plist.contains("<string>com.ajax.web</string>"));
        assert!(plan.plist.contains("<key>KeepAlive</key>"));
        assert!(plan.plist.contains("<key>RunAtLoad</key>"));
        assert!(plan.plist.contains("<key>StandardOutPath</key>"));
        assert!(plan.plist.contains("<key>StandardErrorPath</key>"));
    }

    #[test]
    fn install_writes_launchd_plist_to_disk() {
        let home = temp_home("install-writes-launchd-plist");
        let _ = std::fs::remove_dir_all(&home);
        let config = LaunchdConfig::new(PathBuf::from("/usr/local/bin/ajax"), home.clone());

        let output = super::install_launchd_job(&config).unwrap();

        let plist_path = home
            .join("Library")
            .join("LaunchAgents")
            .join("com.ajax.web.plist");
        let plist = std::fs::read_to_string(&plist_path).unwrap();
        assert!(plist.contains("<string>com.ajax.web</string>"));
        assert!(plist.contains("<string>/usr/local/bin/ajax</string>"));
        assert!(plist.contains("<string>web</string>"));
        assert!(plist.contains("<string>--host</string>"));
        assert!(plist.contains("<string>0.0.0.0</string>"));
        assert!(plist.contains("<string>--port</string>"));
        assert!(plist.contains("<string>8787</string>"));
        assert!(output.contains("installed ajax web server launchd job"));
        assert!(output.contains(&plist_path.display().to_string()));

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn server_start_subcommand_dispatches_launchd_action() {
        let matches = crate::build_cli()
            .try_get_matches_from(["ajax", "server", "start"])
            .unwrap();
        let Some(("server", subcommand)) = matches.subcommand() else {
            panic!("server command should parse");
        };
        let config = launchd_config();
        let mut runner = RecordingCommandRunner::default();

        let rendered =
            render_server_command_with_config(subcommand, &config, "gui/501", &mut runner).unwrap();

        assert_eq!(
            runner.commands(),
            &[
                command(
                    "launchctl",
                    &[
                        "bootstrap",
                        "gui/501",
                        "/Users/matt/Library/LaunchAgents/com.ajax.web.plist",
                    ],
                ),
                command("launchctl", &["kickstart", "-k", "gui/501/com.ajax.web"],),
            ]
        );
        assert!(rendered.output.contains("started ajax web server"));
        assert!(!rendered.state_changed);
    }

    fn launchd_config() -> LaunchdConfig {
        LaunchdConfig::new(
            PathBuf::from("/usr/local/bin/ajax"),
            PathBuf::from("/Users/matt"),
        )
    }

    fn command(program: &str, args: &[&str]) -> CommandSpec {
        CommandSpec {
            program: program.to_string(),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            cwd: None,
            mode: ajax_core::adapters::CommandMode::Capture,
        }
    }

    fn temp_home(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ajax-cli-{name}-{}", std::process::id()))
    }
}
