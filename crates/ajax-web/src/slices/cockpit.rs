//! Browser Cockpit read experience.

use ajax_core::{
    commands::{self, CommandContext},
    output::{InboxResponse, ReposResponse, TaskCard},
    registry::Registry,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct BrowserCockpitView {
    pub repos: ReposResponse,
    pub cards: Vec<BrowserTaskCard>,
    pub inbox: InboxResponse,
}

#[derive(Serialize)]
pub struct BrowserTaskCard {
    pub id: String,
    pub qualified_handle: String,
    pub title: String,
    pub ui_state: String,
    pub status_label: String,
    pub lifecycle: String,
    pub primary_action: String,
    pub available_actions: Vec<String>,
    pub live_summary: Option<String>,
}

pub fn browser_cockpit_json<R: Registry>(
    context: &CommandContext<R>,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&browser_cockpit_view(context))
}

pub fn browser_cockpit_view<R: Registry>(context: &CommandContext<R>) -> BrowserCockpitView {
    let view = commands::rebuild_cockpit_view(context);
    BrowserCockpitView {
        repos: view.repos,
        cards: view.cards.iter().map(browser_task_card).collect(),
        inbox: view.inbox,
    }
}

fn browser_task_card(card: &TaskCard) -> BrowserTaskCard {
    BrowserTaskCard {
        id: card.id.as_str().to_string(),
        qualified_handle: card.qualified_handle.clone(),
        title: card.title.clone(),
        ui_state: card.ui_state.as_str().to_string(),
        status_label: card.status_label.clone(),
        lifecycle: format!("{:?}", card.lifecycle),
        primary_action: card.primary_action.as_str().to_string(),
        available_actions: card
            .available_actions
            .iter()
            .map(|action| action.as_str().to_string())
            .collect(),
        live_summary: card.live_summary.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::browser_cockpit_json;
    use ajax_core::{commands::CommandContext, config::Config, registry::InMemoryRegistry};

    #[test]
    fn cockpit_slice_serializes_empty_projection() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let json = browser_cockpit_json(&context).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["repos"]["repos"], serde_json::json!([]));
        assert_eq!(value["cards"], serde_json::json!([]));
        assert_eq!(value["inbox"]["items"], serde_json::json!([]));
    }
}
