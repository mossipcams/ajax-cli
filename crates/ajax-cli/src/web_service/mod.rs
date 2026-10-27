use ajax_core::adapters::{
    CommandMode, CommandOutput, CommandRunner, CommandSpec, ProcessCommandRunner,
};
use ajax_web::adapters::server::{DEFAULT_STABLE_PORT, DEV_PROFILE};
use ajax_web::slices::dev_deploy::DEV_PORT;
use clap::parser::ValueSource;
use clap::ArgMatches;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::CliError;

#[cfg(test)]
mod tests;

pub(crate) const DEFAULT_WEB_HOST: &str = "0.0.0.0";
pub(crate) const DEFAULT_WEB_PORT: &str = "8787";

/// Copied from the installing shell when set. The restart profile and port are never copied; they describe the installed job.
const FORWARDED_ENV_KEYS: [&str; 10] = [
    "PATH",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "SHELL",
    "AJAX_SANDBOX",
    "AJAX_HOME",
    "AJAX_CONFIG",
    "AJAX_STATE",
    "AJAX_WORKTREE_ROOT",
];
const RESTART_SCRIPT_ENV: &str = "AJAX_WEB_RESTART_SCRIPT";
const RESTART_PROFILE_ENV: &str = "AJAX_WEB_RESTART_PROFILE";
const RESTART_PORT_ENV: &str = "AJAX_WEB_RESTART_PORT";
const SUPERVISOR_ENV: (&str, &str) = ("AJAX_WEB_SUPERVISOR", "launchd");
const UNLOAD_TIMEOUT: Duration = Duration::from_secs(5);
const UNLOAD_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, Eq, PartialEq)]
struct ServiceConfig {
    profile: String,
    host: String,
    port: String,
    ajax_executable: PathBuf,
    /// Global path flags given explicitly on the install invocation, e.g. `--home <path>`.
    global_args: Vec<String>,
    home_dir: PathBuf,
    working_dir: PathBuf,
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
        let mut arguments = vec![self.ajax_executable.display().to_string()];
        arguments.extend(self.global_args.iter().cloned());
        arguments.extend([
            "--profile".to_string(),
            self.profile.clone(),
            "web".to_string(),
            "--host".to_string(),
            self.host.clone(),
            "--port".to_string(),
            self.port.clone(),
        ]);
        arguments
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
        let working_dir = escape_plist_value(&self.working_dir.display().to_string());
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
<string>{working_dir}</string>
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

/// Global path flags typed on this invocation, forwarded so the job resolves the same paths.
pub(crate) fn explicit_path_args(matches: &ArgMatches) -> Vec<String> {
    ["home", "config", "state", "worktree-root"]
        .into_iter()
        .filter(|flag| matches.value_source(flag) == Some(ValueSource::CommandLine))
        .flat_map(|flag| {
            let value = matches.get_one::<String>(flag).cloned().unwrap_or_default();
            [format!("--{flag}"), value]
        })
        .collect()
}

pub(crate) fn run_web_service_command(
    service: &ArgMatches,
    profile: &str,
    global_args: &[String],
) -> Result<String, CliError> {
    if !cfg!(target_os = "macos") {
        return Err(CliError::CommandFailed(
            "launchd service management requires macOS".to_string(),
        ));
    }

    let install = service.subcommand_matches("install");
    let install_arg =
        |name: &str| install.and_then(|matches| matches.get_one::<String>(name).cloned());
    let home_dir = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| CliError::CommandFailed("HOME is required".to_string()))?;
    let port = install_arg("port").unwrap_or_else(|| default_port_for(profile));
    let config = ServiceConfig {
        profile: profile.to_string(),
        host: install_arg("host").unwrap_or_else(|| DEFAULT_WEB_HOST.to_string()),
        env: forwarded_env(profile, &port, |key| std::env::var(key).ok()),
        port,
        ajax_executable: std::env::current_exe()
            .map_err(|error| CliError::CommandFailed(format!("could not locate ajax: {error}")))?,
        global_args: global_args.to_vec(),
        // The restart script is found by searching up from the working directory.
        working_dir: std::env::current_dir().unwrap_or_else(|_| home_dir.clone()),
        home_dir,
    };
    let mut runner = ProcessCommandRunner;
    let domain = current_launchd_domain(&mut runner)?;

    run_web_service_action(service, &config, &domain, &mut runner)
}

/// The dev profile keeps its own port so `web service install` does not collide with stable.
fn default_port_for(profile: &str) -> String {
    if profile == DEV_PROFILE {
        DEV_PORT.to_string()
    } else {
        DEFAULT_STABLE_PORT.to_string()
    }
}

/// Environment baked into the plist. The restart profile and port always describe the installed job.
fn forwarded_env(
    profile: &str,
    port: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = FORWARDED_ENV_KEYS
        .iter()
        .filter_map(|key| Some((key.to_string(), lookup(key)?)))
        .collect();
    if let Some(script) = lookup(RESTART_SCRIPT_ENV).filter(|script| Path::new(script).is_file()) {
        env.push((RESTART_SCRIPT_ENV.to_string(), script));
    }
    env.push((RESTART_PROFILE_ENV.to_string(), profile.to_string()));
    env.push((RESTART_PORT_ENV.to_string(), port.to_string()));
    env
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
    install_with_unload_timeout(config, domain, runner, UNLOAD_TIMEOUT)
}

fn install_with_unload_timeout(
    config: &ServiceConfig,
    domain: &str,
    runner: &mut impl CommandRunner,
    unload_timeout: Duration,
) -> Result<String, CliError> {
    validate_bind(&config.host, &config.port)?;
    write_plist(config)?;
    let target = service_target(domain, &config.label());
    let plist = config.plist_path().display().to_string();

    // A first install has nothing loaded to boot out; a re-install replaces the job.
    run_launchctl_tolerant(runner, &["bootout", &target])?;
    wait_until_unloaded(runner, &target, unload_timeout)?;
    run_launchctl(runner, &["bootstrap", domain, &plist])?;

    Ok(format!("installed {} at {plist}\n", config.label()))
}

/// Rejects bad bind values before the plist is written or launchctl is called.
fn validate_bind(host: &str, port: &str) -> Result<(), CliError> {
    if host.trim().is_empty() {
        return Err(CliError::CommandFailed(
            "invalid --host value: must not be empty".to_string(),
        ));
    }
    match port.parse::<u16>() {
        Ok(value) if value > 0 => Ok(()),
        _ => Err(CliError::CommandFailed(format!(
            "invalid --port value: {port} (expected 1-65535)"
        ))),
    }
}

/// `bootout` returns before launchd finishes unloading, and a `bootstrap` in that window fails.
fn wait_until_unloaded(
    runner: &mut impl CommandRunner,
    target: &str,
    timeout: Duration,
) -> Result<(), CliError> {
    let deadline = Instant::now() + timeout;
    while is_loaded(runner, target)? {
        if Instant::now() >= deadline {
            return Err(CliError::CommandFailed(format!(
                "launchd still has {target} loaded after {} ms; retry the install",
                timeout.as_millis()
            )));
        }
        std::thread::sleep(UNLOAD_POLL_INTERVAL);
    }
    Ok(())
}

fn is_loaded(runner: &mut impl CommandRunner, target: &str) -> Result<bool, CliError> {
    Ok(run_launchctl_tolerant(runner, &["print", target])?.is_some())
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

    if is_loaded(runner, &target)? {
        run_launchctl(runner, &["kickstart", &target])?;
    } else {
        // RunAtLoad starts a freshly bootstrapped job, so no kickstart is needed.
        run_launchctl(runner, &["bootstrap", domain, &plist])?;
    }

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

fn current_launchd_domain(runner: &mut impl CommandRunner) -> Result<String, CliError> {
    let output = runner
        .run(&CommandSpec::new("id", ["-u"]))
        .map_err(|error| {
            CliError::CommandFailed(format!("could not determine user id: {error}"))
        })?;
    let uid = output.stdout.trim();
    if output.status_code != 0 || uid.is_empty() {
        return Err(CliError::CommandFailed(format!(
            "could not determine user id: {}",
            output.stderr.trim()
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
