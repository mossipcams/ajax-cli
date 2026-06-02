use ajax_core::{
    adapters::CommandRunner,
    slices::pane::{send_keys, PaneError, SendKeysOutcome},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TmuxInputAdapter;

impl TmuxInputAdapter {
    pub fn send_keys(
        &self,
        runner: &mut impl CommandRunner,
        session: &str,
        keys: &str,
        submit: bool,
    ) -> Result<SendKeysOutcome, PaneError> {
        send_keys(runner, session, keys, submit)
    }
}

#[cfg(test)]
mod tests {
    use super::TmuxInputAdapter;
    use ajax_core::adapters::RecordingCommandRunner;

    #[test]
    fn tmux_input_adapter_builds_send_keys_commands_for_literal_and_control_input() {
        let adapter = TmuxInputAdapter;
        let mut runner = RecordingCommandRunner::default();

        adapter
            .send_keys(&mut runner, "ajax-web-fix-login", "approve it", true)
            .unwrap();
        adapter
            .send_keys(&mut runner, "ajax-web-fix-login", "C-c", false)
            .unwrap();

        let commands = runner.commands();
        assert_eq!(
            commands[0].args,
            vec![
                "send-keys",
                "-t",
                "ajax-web-fix-login:worktrunk",
                "approve it",
                "Enter",
            ]
        );
        assert_eq!(
            commands[1].args,
            vec!["send-keys", "-t", "ajax-web-fix-login:worktrunk", "C-c",]
        );
    }
}
