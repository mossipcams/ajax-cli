use ajax_core::adapters::{
    CommandMode, CommandOutput, CommandRunner, CommandSpec, ProcessCommandRunner,
};
use clap::ArgMatches;
use std::path::PathBuf;

use crate::CliError;

pub(crate) const DEFAULT_WEB_HOST: &str = "0.0.0.0";
pub(crate) const DEFAULT_WEB_PORT: &str = "8787";

const ENV_KEYS: [&str; 4] = [
    "PATH",
    "AJAX_WEB_RESTART_SCRIPT",
    "AJAX_WEB_RESTART_PROFILE",
    "AJAX_WEB_RESTART_PORT",
];
const SUPERVISOR_ENV: (&str, &str) = ("AJAX_WEB_SUPERVISOR", "launchd");

#[derive(Clone, Debug, Eq, PartialEq)]
struct ServiceConfig {
    profile: String,
    host: String,
    port: String,
    ajax_executable: PathBuf,
    home_dir: PathBuf,
    env: Vec<(String, String)>,
}

impl ServiceConfig {
    fn label(&self) -> String {
        format!("com.ajax.web.{}", self.profile)
    }

    fn plist_path(&self) -> PathBuf {
        self.home_dir
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", self.label()))
    }

    fn log_path(&self) -> PathBuf {
        self.home_dir
            .join("Library/Logs")
            .join(format!("ajax-web-{}.log", self.profile))
    }

    fn program_arguments(&self) -> Vec<String> {
        [
            self.ajax_executable.display().to_string(),
            "--profile".to_string(),
            self.profile.clone(),
            "web".to_string(),
            "--host".to_string(),
            self.host.clone(),
            "--port".to_string(),
            self.port.clone(),
        ]
        .to_vec()
    }

    fn render_plist(&self) -> String {
        let arguments = self
            .program_arguments()
            .iter()
            .map(|argument| format!("<string>{}</string>", escape_plist_value(argument)))
            .collect::<Vec<_>>()
            .join("\n");
        let entries: String = self
            .env
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .chain(std::iter::once(SUPERVISOR_ENV))
            .map(|(key, value)| {
                let (key, value) = (escape_plist_value(key), escape_plist_value(value));
                format!("<key>{key}</key>\n<string>{value}</string>\n")
            })
            .collect();
        let environment = format!("<key>EnvironmentVariables</key>\n<dict>\n{entries}</dict>\n");
        let label = escape_plist_value(&self.label());
        let home = escape_plist_value(&self.home_dir.display().to_string());
        let log = escape_plist_value(&self.log_path().display().to_string());

        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
<key>Label</key>
<string>{label}</string>
<key>ProgramArguments</key>
<array>
{arguments}
</array>
{environment}<key>RunAtLoad</key>
<true/>
<key>KeepAlive</key>
<true/>
<key>AbandonProcessGroup</key>
<true/>
<key>ThrottleInterval</key>
<integer>10</integer>
<key>WorkingDirectory</key>
<string>{home}</string>
<key>StandardOutPath</key>
<string>{log}</string>
<key>StandardErrorPath</key>
<string>{log}</string>
</dict>
</plist>
"#
        )
    }
}

/// The `ajax web service ...` matches, when that route was requested.
pub(crate) fn service_subcommand(matches: &ArgMatches) -> Option<&ArgMatches> {
    matches
        .subcommand_matches("web")?
        .subcommand_matches("service")
}

pub(crate) fn run_web_service_command(
    service: &ArgMatches,
    profile: &str,
) -> Result<String, CliError> {
    if !cfg!(target_os = "macos") {
        return Err(CliError::CommandFailed(
            "launchd service management requires macOS".to_string(),
        ));
    }

    let install = service.subcommand_matches("install");
    let bind_arg = |name: &str, default: &str| {
        install
            .and_then(|matches| matches.get_one::<String>(name).cloned())
            .unwrap_or_else(|| default.to_string())
    };
    let config = ServiceConfig {
        profile: profile.to_string(),
        host: bind_arg("host", DEFAULT_WEB_HOST),
        port: bind_arg("port", DEFAULT_WEB_PORT),
        ajax_executable: std::env::current_exe()
            .map_err(|error| CliError::CommandFailed(format!("could not locate ajax: {error}")))?,
        home_dir: std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| CliError::CommandFailed("HOME is required".to_string()))?,
        env: ENV_KEYS
            .iter()
            .filter_map(|key| Some((key.to_string(), std::env::var(key).ok()?)))
            .collect(),
    };
    let domain = current_launchd_domain()?;

    run_web_service_action(service, &config, &domain, &mut ProcessCommandRunner)
}

fn run_web_service_action(
    service: &ArgMatches,
    config: &ServiceConfig,
    domain: &str,
    runner: &mut impl CommandRunner,
) -> Result<String, CliError> {
    match service.subcommand() {
        Some(("install", _)) => install(config, domain, runner),
        Some(("uninstall", _)) => uninstall(config, domain, runner),
        Some(("start", _)) => start(config, domain, runner),
        Some(("stop", _)) => stop(config, domain, runner),
        Some(("status", _)) => status(config, domain, runner),
        _ => Err(CliError::CommandFailed(
            "web service subcommand is required".to_string(),
        )),
    }
}

fn install(
    config: &ServiceConfig,
    domain: &str,
    runner: &mut impl CommandRunner,
) -> Result<String, CliError> {
    write_plist(config)?;
    let target = service_target(domain, &config.label());
    let plist = config.plist_path().display().to_string();

    // A first install has nothing loaded to boot out; a re-install replaces the job.
    run_launchctl_tolerant(runner, &["bootout", &target])?;
    run_launchctl(runner, &["bootstrap", domain, &plist])?;

    Ok(format!("installed {} at {plist}\n", config.label()))
}

fn uninstall(
    config: &ServiceConfig,
    domain: &str,
    runner: &mut impl CommandRunner,
) -> Result<String, CliError> {
    let target = service_target(domain, &config.label());
    run_launchctl_tolerant(runner, &["bootout", &target])?;

    let plist = config.plist_path();
    // A missing plist is already uninstalled.
    match std::fs::remove_file(&plist) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(CliError::CommandFailed(format!(
                "could not remove launchd plist {}: {error}",
                plist.display()
            )))
        }
        _ => Ok(format!("uninstalled {}\n", config.label())),
    }
}

fn start(
    config: &ServiceConfig,
    domain: &str,
    runner: &mut impl CommandRunner,
) -> Result<String, CliError> {
    let plist = config.plist_path();
    if !plist.is_file() {
        return Err(CliError::CommandFailed(format!(
            "ajax web service {} is not installed; run `ajax web service install`",
            config.label()
        )));
    }
    let plist = plist.display().to_string();
    let target = service_target(domain, &config.label());

    // Already loaded is fine; kickstart reports the real failure if the job is unusable.
    run_launchctl_tolerant(runner, &["bootstrap", domain, &plist])?;
    run_launchctl(runner, &["kickstart", &target])?;

    Ok(format!("started {}\n", config.label()))
}

fn stop(
    config: &ServiceConfig,
    domain: &str,
    runner: &mut impl CommandRunner,
) -> Result<String, CliError> {
    // Bootout rather than kill: KeepAlive would respawn a killed job.
    let target = service_target(domain, &config.label());
    run_launchctl_tolerant(runner, &["bootout", &target])?;

    Ok(format!("stopped {}\n", config.label()))
}

fn status(
    config: &ServiceConfig,
    domain: &str,
    runner: &mut impl CommandRunner,
) -> Result<String, CliError> {
    let target = service_target(domain, &config.label());
    let print = run_launchctl_tolerant(runner, &["print", &target])?;

    let yes_no = |value: bool| if value { "yes" } else { "no" };
    let mut lines = vec![
        format!("label: {}", config.label()),
        format!("installed: {}", yes_no(config.plist_path().is_file())),
        format!("loaded: {}", yes_no(print.is_some())),
    ];
    if let Some(output) = print {
        lines.extend(
            output
                .stdout
                .lines()
                .map(str::trim)
                .filter(|line| line.starts_with("state = ") || line.starts_with("pid = "))
                .map(str::to_string),
        );
    }

    Ok(lines.join("\n") + "\n")
}

fn write_plist(config: &ServiceConfig) -> Result<(), CliError> {
    let plist = config.plist_path();
    let log = config.log_path();
    for dir in [plist.parent(), log.parent()].into_iter().flatten() {
        std::fs::create_dir_all(dir).map_err(|error| {
            CliError::CommandFailed(format!("could not create {}: {error}", dir.display()))
        })?;
    }
    std::fs::write(&plist, config.render_plist()).map_err(|error| {
        CliError::CommandFailed(format!(
            "could not write launchd plist {}: {error}",
            plist.display()
        ))
    })
}

fn run_launchctl(
    runner: &mut impl CommandRunner,
    args: &[&str],
) -> Result<CommandOutput, CliError> {
    let output = run_launchctl_raw(runner, args)?;
    if output.status_code != 0 {
        return Err(CliError::CommandFailed(format!(
            "launchctl {} failed: {}",
            args.join(" "),
            output.stderr.trim()
        )));
    }
    Ok(output)
}

/// `launchctl` exits non-zero for "not loaded" and "already loaded"; those are
/// expected here, so a non-zero exit maps to `None` and other failures still surface.
fn run_launchctl_tolerant(
    runner: &mut impl CommandRunner,
    args: &[&str],
) -> Result<Option<CommandOutput>, CliError> {
    let output = run_launchctl_raw(runner, args)?;
    Ok((output.status_code == 0).then_some(output))
}

fn run_launchctl_raw(
    runner: &mut impl CommandRunner,
    args: &[&str],
) -> Result<CommandOutput, CliError> {
    runner
        .run(&launchctl_spec(args))
        .map_err(|error| CliError::CommandFailed(format!("launchctl failed: {error}")))
}

fn launchctl_spec(args: &[&str]) -> CommandSpec {
    CommandSpec {
        program: "launchctl".to_string(),
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
        cwd: None,
        mode: CommandMode::Capture,
        timeout: None,
    }
}

fn service_target(domain: &str, label: &str) -> String {
    format!("{domain}/{label}")
}

fn current_launchd_domain() -> Result<String, CliError> {
    let output = std::process::Command::new("id")
        .arg("-u")
        .output()
        .map_err(|error| {
            CliError::CommandFailed(format!("could not determine user id: {error}"))
        })?;
    let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || uid.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(CliError::CommandFailed(format!(
            "could not determine user id: {stderr}"
        )));
    }

    Ok(format!("gui/{uid}"))
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
    use ajax_core::adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec};
    use std::path::{Path, PathBuf};

    use super::{install, run_web_service_action, start, uninstall, ServiceConfig};

    #[test]
    fn plist_runs_profile_web_server_at_login_and_respawns_after_any_exit() {
        let plist = service_config().render_plist();

        assert_eq!(
            service_config().program_arguments().join(" "),
            "/usr/local/bin/ajax --profile dev web --host 0.0.0.0 --port 8787"
        );
        assert!(plist.contains("<key>Label</key>\n<string>com.ajax.web.dev</string>"));
        assert!(plist.contains("<string>--profile</string>\n<string>dev</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n<true/>"));
        assert!(plist.contains("<key>KeepAlive</key>\n<true/>"));
        assert!(plist.contains("<key>AbandonProcessGroup</key>\n<true/>"));
        assert!(plist.contains("<key>AJAX_WEB_SUPERVISOR</key>\n<string>launchd</string>"));
        assert!(plist.contains("<key>ThrottleInterval</key>\n<integer>10</integer>"));
        assert!(plist.contains("<string>/Users/matt/Library/Logs/ajax-web-dev.log</string>"));
    }

    #[test]
    fn plist_embeds_path_and_escapes_values() {
        let config = ServiceConfig {
            ajax_executable: PathBuf::from("/opt/a&b/<ajax>"),
            env: vec![("PATH".to_string(), "/usr/bin:\"x\"".to_string())],
            ..service_config()
        };

        let plist = config.render_plist();

        assert!(plist.contains("<string>/opt/a&amp;b/&lt;ajax&gt;</string>"));
        assert!(plist.contains(
            "<key>EnvironmentVariables</key>\n<dict>\n<key>PATH</key>\n<string>/usr/bin:&quot;x&quot;</string>\n<key>AJAX_WEB_SUPERVISOR</key>\n<string>launchd</string>\n</dict>"
        ));
    }

    #[test]
    fn install_writes_plist_then_boots_out_and_bootstraps() {
        let config = temp_config("install-order");
        let mut runner = FakeLaunchctl::default();

        let output = install(&config, "gui/501", &mut runner).unwrap();

        let plist = config.plist_path().display().to_string();
        assert!(std::fs::read_to_string(&plist)
            .unwrap()
            .contains("com.ajax.web.dev"));
        assert_eq!(
            runner.commands,
            vec![
                command(["bootout", "gui/501/com.ajax.web.dev"]),
                command(["bootstrap", "gui/501", &plist]),
            ]
        );
        assert!(output.contains("installed com.ajax.web.dev"));
    }

    #[test]
    fn install_fails_when_bootstrap_exits_non_zero() {
        let config = temp_config("install-bootstrap-fails");
        let mut runner = FakeLaunchctl {
            failing_verb: Some("bootstrap"),
            ..FakeLaunchctl::default()
        };

        assert!(install(&config, "gui/501", &mut runner).is_err());
    }

    #[test]
    fn uninstall_removes_plist_and_tolerates_not_loaded() {
        let config = temp_config("uninstall-removes");
        install(&config, "gui/501", &mut FakeLaunchctl::default()).unwrap();
        let mut runner = FakeLaunchctl {
            failing_verb: Some("bootout"),
            ..FakeLaunchctl::default()
        };

        uninstall(&config, "gui/501", &mut runner).unwrap();

        assert!(!config.plist_path().exists());
        assert_eq!(
            runner.commands,
            vec![command(["bootout", "gui/501/com.ajax.web.dev"])]
        );
    }

    #[test]
    fn start_errors_when_not_installed() {
        let config = temp_config("start-not-installed");
        let mut runner = FakeLaunchctl::default();

        let error = start(&config, "gui/501", &mut runner).unwrap_err();

        assert!(error.to_string().contains("ajax web service install"));
        assert!(runner.commands.is_empty());
    }

    #[test]
    fn start_bootstraps_and_kickstarts_installed_job() {
        let config = temp_config("start-installed");
        install(&config, "gui/501", &mut FakeLaunchctl::default()).unwrap();
        let mut runner = FakeLaunchctl::default();
        let plist = config.plist_path().display().to_string();

        start(&config, "gui/501", &mut runner).unwrap();

        assert_eq!(
            runner.commands,
            vec![
                command(["bootstrap", "gui/501", &plist]),
                command(["kickstart", "gui/501/com.ajax.web.dev"]),
            ]
        );
    }

    #[test]
    fn stop_boots_out_job_so_keepalive_does_not_respawn() {
        let mut runner = FakeLaunchctl::default();

        let output = run("stop", &service_config(), &mut runner);

        assert_eq!(
            runner.commands,
            vec![command(["bootout", "gui/501/com.ajax.web.dev"])]
        );
        assert!(output.contains("stopped com.ajax.web.dev"));
    }

    #[test]
    fn status_reports_not_loaded_without_error() {
        let mut runner = FakeLaunchctl {
            failing_verb: Some("print"),
            ..FakeLaunchctl::default()
        };

        let output = run("status", &temp_config("status-not-loaded"), &mut runner);

        assert_eq!(
            output,
            "label: com.ajax.web.dev\ninstalled: no\nloaded: no\n"
        );
    }

    #[test]
    fn status_reports_state_and_pid_when_loaded() {
        let mut runner = FakeLaunchctl {
            stdout: "\tstate = running\n\tpid = 4242\n".to_string(),
            ..FakeLaunchctl::default()
        };

        let output = run("status", &service_config(), &mut runner);

        assert!(output.ends_with("loaded: yes\nstate = running\npid = 4242\n"));
    }

    #[test]
    fn web_service_and_plain_web_command_both_parse() {
        let matches = crate::build_cli()
            .try_get_matches_from(["ajax", "web", "service", "install", "--port", "9999"])
            .unwrap();
        let install = super::service_subcommand(&matches)
            .and_then(|service| service.subcommand_matches("install"))
            .unwrap();
        assert_eq!(install.get_one::<String>("port").unwrap(), "9999");

        let matches = crate::build_cli()
            .try_get_matches_from(["ajax", "web", "--port", "8788"])
            .unwrap();
        let web = matches.subcommand_matches("web").unwrap();
        assert_eq!(web.get_one::<String>("port").unwrap(), "8788");
        assert!(super::service_subcommand(&matches).is_none());
    }

    #[derive(Default)]
    struct FakeLaunchctl {
        commands: Vec<CommandSpec>,
        failing_verb: Option<&'static str>,
        stdout: String,
    }

    impl CommandRunner for FakeLaunchctl {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            // Mirrors ProcessCommandRunner: a non-zero exit is an Ok output, not an Err.
            let failing = command.args.first().map(String::as_str) == self.failing_verb;
            Ok(CommandOutput {
                status_code: if failing { 113 } else { 0 },
                stdout: self.stdout.clone(),
                stderr: String::new(),
            })
        }
    }

    fn service_config() -> ServiceConfig {
        ServiceConfig {
            profile: "dev".to_string(),
            host: "0.0.0.0".to_string(),
            port: "8787".to_string(),
            ajax_executable: PathBuf::from("/usr/local/bin/ajax"),
            home_dir: PathBuf::from("/Users/matt"),
            env: Vec::new(),
        }
    }

    fn run(name: &str, config: &ServiceConfig, runner: &mut FakeLaunchctl) -> String {
        let matches = crate::build_cli()
            .try_get_matches_from(["ajax", "web", "service", name])
            .unwrap();
        let service = super::service_subcommand(&matches).unwrap();
        run_web_service_action(service, config, "gui/501", runner).unwrap()
    }

    fn command<const N: usize>(args: [&str; N]) -> CommandSpec {
        CommandSpec::new("launchctl", args)
    }

    /// Scratch home under the worktree's target/ dir, never the real HOME or /tmp.
    fn temp_config(name: &str) -> ServiceConfig {
        let home = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/ajax-web-service-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        ServiceConfig {
            home_dir: home,
            ..service_config()
        }
    }
}
