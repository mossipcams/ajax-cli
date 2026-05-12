use ajax_core::adapters::CommandRunError;

pub(crate) fn command_error_looks_conflicted(error: &CommandRunError) -> bool {
    match error {
        CommandRunError::NonZeroExit { stderr, .. } => {
            stderr.to_ascii_lowercase().contains("conflict")
        }
        CommandRunError::SpawnFailed(_) | CommandRunError::MissingStatusCode => false,
    }
}

#[cfg(test)]
mod tests {
    use ajax_core::adapters::CommandRunError;

    use super::command_error_looks_conflicted;

    #[test]
    fn command_conflict_classifier_only_matches_nonzero_conflict_stderr() {
        let conflicted = CommandRunError::NonZeroExit {
            program: "git".to_string(),
            status_code: 1,
            stderr: "Automatic merge failed; fix conflicts and then commit.".to_string(),
            cwd: None,
        };
        let unrelated_nonzero = CommandRunError::NonZeroExit {
            program: "git".to_string(),
            status_code: 1,
            stderr: "fatal: not possible to fast-forward, aborting.".to_string(),
            cwd: None,
        };

        assert!(command_error_looks_conflicted(&conflicted));
        assert!(!command_error_looks_conflicted(&unrelated_nonzero));
        assert!(!command_error_looks_conflicted(
            &CommandRunError::MissingStatusCode
        ));
        assert!(!command_error_looks_conflicted(
            &CommandRunError::SpawnFailed("missing git".to_string())
        ));
    }
}
