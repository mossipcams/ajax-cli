use ajax_core::adapters::CommandRunError;

pub(crate) fn command_error_looks_conflicted(error: &CommandRunError) -> bool {
    match error {
        CommandRunError::NonZeroExit { stderr, .. } => {
            stderr.to_ascii_lowercase().contains("conflict")
        }
        CommandRunError::SpawnFailed(_) | CommandRunError::MissingStatusCode => false,
    }
}
