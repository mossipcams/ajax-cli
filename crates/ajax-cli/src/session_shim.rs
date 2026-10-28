use std::{ffi::OsString, path::PathBuf};

use ajax_web::adapters::session_shim::{run_session_shim, ShimArgs};
use clap::ArgMatches;

use crate::CliError;

pub(crate) fn run_session_shim_command(matches: &ArgMatches) -> Result<i32, CliError> {
    let handle = matches
        .get_one::<String>("handle")
        .cloned()
        .ok_or_else(|| CliError::CommandFailed("session shim handle is required".to_string()))?;
    let state_dir = matches
        .get_one::<PathBuf>("state-dir")
        .cloned()
        .ok_or_else(|| CliError::CommandFailed("session shim state dir is required".to_string()))?;
    let argv = matches
        .get_many::<OsString>("program")
        .map(|values| values.cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    run_session_shim(ShimArgs {
        handle,
        state_dir,
        argv,
    })
    .map_err(|error| CliError::CommandFailed(error.to_string()))
}
