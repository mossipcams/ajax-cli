use ajax_core::{
    commands::{self, CommandContext},
    registry::InMemoryRegistry,
};
use ajax_tui::CockpitSnapshot;

pub(crate) fn build_cockpit_snapshot(
    context: &CommandContext<InMemoryRegistry>,
) -> CockpitSnapshot {
    let view = commands::cockpit_view(context);
    CockpitSnapshot {
        repos: view.repos,
        cards: view.cards,
        inbox: view.inbox,
    }
}
