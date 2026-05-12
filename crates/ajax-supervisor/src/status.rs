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
    fn status_machine_reduces_event_batches_without_late_output_override() {
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
            Some(LiveStatusKind::Done)
        );
    }
}
