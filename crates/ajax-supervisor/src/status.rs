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

    pub fn observation(&self) -> Option<&LiveObservation> {
        self.observation.as_ref()
    }
}
