use ajax_web::slices::web_session::cursor_auth::{
    cursor_auth_renew, cursor_auth_sign_in, cursor_auth_sign_out, cursor_auth_status,
};
use clap::ArgMatches;
use std::env;
use std::path::PathBuf;

use crate::CliError;

fn auth_cwd() -> PathBuf {
    env::temp_dir()
}

fn render_json<T: serde::Serialize>(value: &T) -> Result<String, CliError> {
    serde_json::to_string_pretty(value)
        .map_err(|error| CliError::JsonSerialization(format!("cursor auth json failed: {error}")))
}

pub(crate) fn run_cursor_command(subcommand: &ArgMatches) -> Result<String, CliError> {
    match subcommand.subcommand() {
        Some(("auth", auth)) => run_cursor_auth_command(auth),
        _ => Err(CliError::CommandFailed(
            "unknown cursor subcommand (expected: cursor auth …)".to_string(),
        )),
    }
}

fn run_cursor_auth_command(matches: &ArgMatches) -> Result<String, CliError> {
    let cwd = auth_cwd();
    let json = matches.get_flag("json");
    match matches.subcommand() {
        Some(("status", sub)) => {
            let status = cursor_auth_status(&cwd).map_err(CliError::CommandFailed)?;
            if json || sub.get_flag("json") {
                return render_json(&status);
            }
            if status.present {
                let mut line = "Cursor SDK credential: present".to_string();
                if let Some(label) = &status.account_label {
                    line.push_str(&format!(" ({label})"));
                }
                if let Some(expires) = &status.expires_at {
                    line.push_str(&format!("; expires {expires}"));
                }
                Ok(line)
            } else {
                Ok("Cursor SDK credential: absent".to_string())
            }
        }
        Some(("sign-in", sub)) => {
            let result = cursor_auth_sign_in(&cwd).map_err(CliError::CommandFailed)?;
            if json || sub.get_flag("json") {
                return render_json(&result);
            }
            let mut lines = Vec::new();
            if let Some(url) = &result.login_url {
                lines.push(format!("Open this URL to sign in:\n{url}"));
            }
            lines.push(format!("Status: {}", result.status));
            if let Some(label) = &result.account_label {
                lines.push(format!("Account: {label}"));
            }
            if let Some(expires) = &result.expires_at {
                lines.push(format!("Expires: {expires}"));
            }
            Ok(lines.join("\n"))
        }
        Some(("renew", sub)) => {
            let result = cursor_auth_renew(&cwd).map_err(CliError::CommandFailed)?;
            if json || sub.get_flag("json") {
                return render_json(&result);
            }
            let mut lines = Vec::new();
            if let Some(url) = &result.login_url {
                lines.push(format!("Open this URL to renew:\n{url}"));
            }
            lines.push(format!("Status: {}", result.status));
            Ok(lines.join("\n"))
        }
        Some(("sign-out", sub)) => {
            let result = cursor_auth_sign_out(&cwd).map_err(CliError::CommandFailed)?;
            if json || sub.get_flag("json") {
                return render_json(&result);
            }
            Ok(if result.signed_out {
                "Cursor SDK credential removed.".to_string()
            } else {
                "Cursor SDK sign-out completed.".to_string()
            })
        }
        _ => Err(CliError::CommandFailed(
            "unknown cursor auth subcommand (expected: status, sign-in, renew, sign-out)"
                .to_string(),
        )),
    }
}
