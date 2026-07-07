use ajax_core::{
    events::{live_observation_from_event, MonitorEvent},
    live::reduce_live_observation,
    models::LiveObservation,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SupervisorStatusMachine {
    observation: Option<LiveObservation>,
}

impl SupervisorStatusMachine {
    pub fn apply(&mut self, event: &MonitorEvent) -> Option<&LiveObservation> {
        let Some(next) = live_observation_from_event(event) else {
            return self.observation();
        };
        self.observation = Some(reduce_live_observation(self.observation.as_ref(), next));
        self.observation()
    }

    pub fn apply_all<'a>(
        &mut self,
        events: impl IntoIterator<Item = &'a MonitorEvent>,
    ) -> Option<&LiveObservation> {
        for event in events {
            self.apply(event);
        }
        self.observation()
    }

    pub fn observation(&self) -> Option<&LiveObservation> {
        self.observation.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use ajax_core::{
        events::{AgentEvent, MonitorEvent, ProcessEvent},
        models::LiveStatusKind,
    };

    use super::SupervisorStatusMachine;

    #[test]
    fn status_machine_yields_done_to_late_busy_output() {
        let events = [
            MonitorEvent::Agent(AgentEvent::Completed),
            MonitorEvent::Process(ProcessEvent::Stdout {
                line: "late stdout".to_string(),
            }),
        ];
        let mut status = SupervisorStatusMachine::default();

        status.apply_all(&events);

        assert_eq!(
            status.observation().map(|observation| observation.kind),
            Some(LiveStatusKind::CommandRunning)
        );
    }

    #[test]
    fn status_machine_preserves_tests_running_over_generic_activity() {
        let events = [
            MonitorEvent::Agent(AgentEvent::ToolCall {
                name: "shell: cargo test --all-features".to_string(),
            }),
            MonitorEvent::Process(ProcessEvent::Stdout {
                line: "Compiling ajax-core v0.1.0".to_string(),
            }),
        ];
        let mut status = SupervisorStatusMachine::default();

        status.apply_all(&events);

        assert_eq!(
            status.observation().map(|observation| observation.kind),
            Some(LiveStatusKind::TestsRunning)
        );
    }

    #[test]
    fn status_machine_yields_blocked_to_late_busy_output() {
        let events = [
            MonitorEvent::Agent(AgentEvent::Failed {
                message: "manual intervention required; blocked".to_string(),
            }),
            MonitorEvent::Process(ProcessEvent::Stdout {
                line: "still streaming logs".to_string(),
            }),
        ];
        let mut status = SupervisorStatusMachine::default();

        status.apply_all(&events);

        assert_eq!(
            status.observation().map(|observation| observation.kind),
            Some(LiveStatusKind::CommandRunning)
        );
    }

    #[test]
    fn status_machine_maps_hung_process_to_blocked() {
        let events = [MonitorEvent::Process(ProcessEvent::Hung {
            quiet_for: std::time::Duration::from_secs(30),
        })];
        let mut status = SupervisorStatusMachine::default();

        status.apply_all(&events);

        assert_eq!(
            status.observation().map(|observation| observation.kind),
            Some(LiveStatusKind::Blocked)
        );
    }
}
