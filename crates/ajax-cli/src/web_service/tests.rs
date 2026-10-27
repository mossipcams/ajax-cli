use ajax_core::adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{
    current_launchd_domain, default_port_for, explicit_path_args, forwarded_env, install,
    install_with_unload_timeout, run_web_service_action, start, uninstall, ServiceConfig,
};

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
fn plist_restart_profile_and_port_always_describe_the_installed_job() {
    let env = forwarded_env(
        "dev",
        "8788",
        env_from(&[
            ("AJAX_WEB_RESTART_PROFILE", "stable"),
            ("AJAX_WEB_RESTART_PORT", "9999"),
        ]),
    );
    let config = ServiceConfig {
        port: "8788".to_string(),
        env,
        ..service_config()
    };

    let plist = config.render_plist();

    assert!(plist.contains("<key>AJAX_WEB_RESTART_PROFILE</key>\n<string>dev</string>"));
    assert!(plist.contains("<key>AJAX_WEB_RESTART_PORT</key>\n<string>8788</string>"));
    assert_eq!(plist.matches("AJAX_WEB_RESTART_PROFILE").count(), 1);
    assert!(!plist.contains("<string>stable</string>"));
    assert!(!plist.contains("9999"));
}

#[test]
fn restart_script_is_forwarded_only_when_it_is_an_existing_file() {
    let existing = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("Cargo.toml")
        .display()
        .to_string();

    let forwarded = forwarded_env(
        "dev",
        "8788",
        env_from(&[("AJAX_WEB_RESTART_SCRIPT", existing.as_str())]),
    );
    let missing = forwarded_env(
        "dev",
        "8788",
        env_from(&[("AJAX_WEB_RESTART_SCRIPT", "/nonexistent/restart.sh")]),
    );

    assert!(forwarded.contains(&("AJAX_WEB_RESTART_SCRIPT".to_string(), existing)));
    assert!(!missing
        .iter()
        .any(|(key, _)| key == "AJAX_WEB_RESTART_SCRIPT"));
}

#[test]
fn forwarded_env_includes_locale_and_shell_when_present() {
    let env = forwarded_env(
        "dev",
        "8788",
        env_from(&[
            ("PATH", "/usr/bin"),
            ("LANG", "en_US.UTF-8"),
            ("SHELL", "/bin/zsh"),
        ]),
    );

    assert!(env.contains(&("PATH".to_string(), "/usr/bin".to_string())));
    assert!(env.contains(&("LANG".to_string(), "en_US.UTF-8".to_string())));
    assert!(env.contains(&("SHELL".to_string(), "/bin/zsh".to_string())));
    assert!(!env.iter().any(|(key, _)| key == "LC_ALL"));
}

#[test]
fn default_port_is_profile_aware() {
    assert_eq!(default_port_for("dev"), "8788");
    assert_eq!(default_port_for("stable"), "8787");
    assert_eq!(default_port_for("other"), "8787");
}

#[test]
fn working_directory_is_the_install_time_cwd() {
    let config = ServiceConfig {
        working_dir: PathBuf::from("/Users/matt/Desktop/Projects/ajax-cli"),
        ..service_config()
    };

    assert!(config.render_plist().contains(
        "<key>WorkingDirectory</key>\n<string>/Users/matt/Desktop/Projects/ajax-cli</string>"
    ));
}

#[test]
fn explicit_global_flags_are_passed_before_web() {
    let config = ServiceConfig {
        global_args: vec!["--home".to_string(), "/tmp/ajax-dev".to_string()],
        ..service_config()
    };

    assert_eq!(
        config.program_arguments().join(" "),
        "/usr/local/bin/ajax --home /tmp/ajax-dev --profile dev web --host 0.0.0.0 --port 8787"
    );
}

#[test]
fn only_path_flags_typed_on_the_command_line_are_forwarded() {
    let matches = crate::build_cli()
        .try_get_matches_from([
            "ajax",
            "--home",
            "/tmp/ajax-dev",
            "web",
            "service",
            "status",
        ])
        .unwrap();
    assert_eq!(
        explicit_path_args(&matches),
        vec!["--home".to_string(), "/tmp/ajax-dev".to_string()]
    );

    let matches = crate::build_cli()
        .try_get_matches_from(["ajax", "web", "service", "status"])
        .unwrap();
    assert!(explicit_path_args(&matches).is_empty());
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
            command(["print", "gui/501/com.ajax.web.dev"]),
            command(["bootstrap", "gui/501", &plist]),
        ]
    );
    assert!(output.contains("installed com.ajax.web.dev"));
}

#[test]
fn install_polls_print_until_unloaded_then_bootstraps() {
    let config = temp_config("install-polls");
    let mut runner = FakeLaunchctl {
        loaded_prints: 3,
        ..FakeLaunchctl::default()
    };
    let plist = config.plist_path().display().to_string();
    let print = command(["print", "gui/501/com.ajax.web.dev"]);

    install(&config, "gui/501", &mut runner).unwrap();

    assert_eq!(
        runner.commands,
        vec![
            command(["bootout", "gui/501/com.ajax.web.dev"]),
            print.clone(),
            print.clone(),
            print.clone(),
            print,
            command(["bootstrap", "gui/501", &plist]),
        ]
    );
}

#[test]
fn install_errors_when_launchd_keeps_the_old_job_loaded() {
    let config = temp_config("install-stays-loaded");
    let mut runner = FakeLaunchctl {
        loaded_prints: usize::MAX,
        ..FakeLaunchctl::default()
    };

    let error =
        install_with_unload_timeout(&config, "gui/501", &mut runner, Duration::ZERO).unwrap_err();

    assert!(error
        .to_string()
        .contains("still has gui/501/com.ajax.web.dev loaded"));
    assert_eq!(
        runner.commands,
        vec![
            command(["bootout", "gui/501/com.ajax.web.dev"]),
            command(["print", "gui/501/com.ajax.web.dev"]),
        ]
    );
}

#[test]
fn install_rejects_invalid_port_before_writing_or_running_launchctl() {
    for port in ["abc", "0", "70000"] {
        let config = ServiceConfig {
            port: port.to_string(),
            ..temp_config(&format!("invalid-port-{port}"))
        };
        let mut runner = FakeLaunchctl::default();

        let error = install(&config, "gui/501", &mut runner).unwrap_err();

        assert!(error
            .to_string()
            .contains(&format!("invalid --port value: {port}")));
        assert!(!config.plist_path().exists());
        assert!(runner.commands.is_empty());
    }
}

#[test]
fn install_rejects_empty_host_before_writing_or_running_launchctl() {
    let config = ServiceConfig {
        host: " ".to_string(),
        ..temp_config("invalid-host")
    };
    let mut runner = FakeLaunchctl::default();

    assert!(install(&config, "gui/501", &mut runner).is_err());
    assert!(!config.plist_path().exists());
    assert!(runner.commands.is_empty());
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
fn start_kickstarts_a_loaded_job_without_bootstrapping() {
    let config = temp_config("start-loaded");
    install(&config, "gui/501", &mut FakeLaunchctl::default()).unwrap();
    let mut runner = FakeLaunchctl {
        loaded_prints: 1,
        ..FakeLaunchctl::default()
    };

    start(&config, "gui/501", &mut runner).unwrap();

    assert_eq!(
        runner.commands,
        vec![
            command(["print", "gui/501/com.ajax.web.dev"]),
            command(["kickstart", "gui/501/com.ajax.web.dev"]),
        ]
    );
}

#[test]
fn start_bootstraps_an_unloaded_job() {
    let config = temp_config("start-not-loaded");
    install(&config, "gui/501", &mut FakeLaunchctl::default()).unwrap();
    let mut runner = FakeLaunchctl::default();
    let plist = config.plist_path().display().to_string();

    start(&config, "gui/501", &mut runner).unwrap();

    assert_eq!(
        runner.commands,
        vec![
            command(["print", "gui/501/com.ajax.web.dev"]),
            command(["bootstrap", "gui/501", &plist]),
        ]
    );
}

#[test]
fn start_surfaces_bootstrap_stderr_when_not_loaded() {
    let config = temp_config("start-bootstrap-fails");
    install(&config, "gui/501", &mut FakeLaunchctl::default()).unwrap();
    let mut runner = FakeLaunchctl {
        failing_verb: Some("bootstrap"),
        ..FakeLaunchctl::default()
    };

    let error = start(&config, "gui/501", &mut runner).unwrap_err();

    assert!(error.to_string().contains("boom"));
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
        loaded_prints: usize::MAX,
        ..FakeLaunchctl::default()
    };

    let output = run("status", &service_config(), &mut runner);

    assert!(output.ends_with("loaded: yes\nstate = running\npid = 4242\n"));
}

#[test]
fn launchd_domain_comes_from_id_through_the_runner() {
    let mut runner = FakeLaunchctl::default();

    assert_eq!(current_launchd_domain(&mut runner).unwrap(), "gui/501");
    assert_eq!(runner.commands, vec![CommandSpec::new("id", ["-u"])]);
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

#[test]
fn install_port_has_no_clap_default_so_the_profile_default_applies() {
    let matches = crate::build_cli()
        .try_get_matches_from(["ajax", "web", "service", "install"])
        .unwrap();
    let install = super::service_subcommand(&matches)
        .and_then(|service| service.subcommand_matches("install"))
        .unwrap();

    assert!(install.get_one::<String>("port").is_none());
}

#[derive(Default)]
struct FakeLaunchctl {
    commands: Vec<CommandSpec>,
    failing_verb: Option<&'static str>,
    stdout: String,
    loaded_prints: usize,
}

impl CommandRunner for FakeLaunchctl {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        if command.program == "id" {
            return Ok(output(0, "501\n", ""));
        }
        let verb = command.args.first().map(String::as_str);
        if verb == Some("print") && self.loaded_prints > 0 {
            self.loaded_prints -= 1;
            return Ok(output(0, &self.stdout, ""));
        }
        if verb == self.failing_verb || verb == Some("print") {
            return Ok(output(113, "", "boom"));
        }
        Ok(output(0, "", ""))
    }
}

fn output(status_code: i32, stdout: &str, stderr: &str) -> CommandOutput {
    CommandOutput {
        status_code,
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    }
}

fn env_from<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |key| {
        pairs
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.to_string())
    }
}

fn service_config() -> ServiceConfig {
    ServiceConfig {
        profile: "dev".to_string(),
        host: "0.0.0.0".to_string(),
        port: "8787".to_string(),
        ajax_executable: PathBuf::from("/usr/local/bin/ajax"),
        global_args: Vec::new(),
        home_dir: PathBuf::from("/Users/matt"),
        working_dir: PathBuf::from("/Users/matt"),
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
