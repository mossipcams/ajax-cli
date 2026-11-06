use crate::adapters::web_session_acp::SessionClient;

#[derive(Default)]
pub(super) enum AgentSlot {
    #[default]
    Absent,
    Spawning(SessionClient),
    Live(SessionClient),
    Exited(SessionClient),
}

impl AgentSlot {
    pub(super) fn client(&self) -> Option<&SessionClient> {
        match self {
            Self::Absent => None,
            Self::Spawning(client) | Self::Live(client) | Self::Exited(client) => Some(client),
        }
    }

    pub(super) fn client_mut(&mut self) -> Option<&mut SessionClient> {
        match self {
            Self::Absent => None,
            Self::Spawning(client) | Self::Live(client) | Self::Exited(client) => Some(client),
        }
    }

    pub(super) fn take_client(&mut self) -> Option<SessionClient> {
        match std::mem::take(self) {
            Self::Absent => None,
            Self::Spawning(client) | Self::Live(client) | Self::Exited(client) => Some(client),
        }
    }

    pub(super) fn install(&mut self, client: SessionClient) {
        *self = Self::Spawning(client);
    }

    pub(super) fn mark_live(&mut self) {
        *self = match std::mem::take(self) {
            Self::Spawning(client) | Self::Exited(client) => Self::Live(client),
            other => other,
        };
    }

    pub(super) fn mark_exited(&mut self) {
        *self = match std::mem::take(self) {
            Self::Spawning(client) | Self::Live(client) => Self::Exited(client),
            other => other,
        };
    }

    pub(super) fn is_live(&self) -> bool {
        matches!(self, Self::Live(_))
    }
}

#[cfg(test)]
mod tests {
    use super::AgentSlot;

    #[test]
    fn absent_slot_holds_no_client_and_is_not_live() {
        let mut slot = AgentSlot::default();

        assert!(slot.client().is_none());
        assert!(slot.client_mut().is_none());
        assert!(!slot.is_live());
        assert!(slot.take_client().is_none());
        assert!(matches!(slot, AgentSlot::Absent));
    }

    #[test]
    fn transitions_leave_absent_slot_absent() {
        let mut slot = AgentSlot::Absent;

        slot.mark_live();
        assert!(matches!(slot, AgentSlot::Absent));
        slot.mark_exited();
        assert!(matches!(slot, AgentSlot::Absent));
        assert!(!slot.is_live());
    }
}
