//! Browser Cockpit read experience.

use ajax_core::{
    commands::{self, CommandContext},
    models::OperatorAction,
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
    let available: Vec<OperatorAction> = card
        .available_actions
        .iter()
        .copied()
        .filter(|action| is_web_supported(*action))
        .collect();
    let primary = if is_web_supported(card.primary_action) {
        card.primary_action
    } else {
        available.first().copied().unwrap_or(card.primary_action)
    };

    BrowserTaskCard {
        id: card.id.as_str().to_string(),
        qualified_handle: card.qualified_handle.clone(),
        title: card.title.clone(),
        ui_state: card.ui_state.as_str().to_string(),
        status_label: card.status_label.clone(),
        lifecycle: format!("{:?}", card.lifecycle),
        primary_action: primary.as_str().to_string(),
        available_actions: available
            .iter()
            .map(|action| action.as_str().to_string())
            .collect(),
        live_summary: card.live_summary.clone(),
    }
}

// Resume drops the operator into a native tmux pane and Start needs an
// interactive title prompt; both are rejected by web action handling, so the
// browser Cockpit should not surface them as buttons.
fn is_web_supported(action: OperatorAction) -> bool {
    !matches!(action, OperatorAction::Resume | OperatorAction::Start)
}

#[cfg(test)]
mod tests {
    use super::{browser_cockpit_json, browser_task_card};
    use ajax_core::{
        commands::CommandContext,
        config::Config,
        models::{LifecycleStatus, OperatorAction, TaskId},
        output::TaskCard,
        registry::InMemoryRegistry,
        ui_state::UiState,
    };

    #[test]
    fn cockpit_slice_serializes_empty_projection() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let json = browser_cockpit_json(&context).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["repos"]["repos"], serde_json::json!([]));
        assert_eq!(value["cards"], serde_json::json!([]));
        assert_eq!(value["inbox"]["items"], serde_json::json!([]));
    }

    #[test]
    fn cockpit_slice_shapes_cards_for_the_mobile_pwa() {
        let card = TaskCard {
            id: TaskId::new("web/fix-login"),
            qualified_handle: "web/fix-login".to_string(),
            title: "Fix login".to_string(),
            ui_state: UiState::ReviewReady,
            status_label: "review ready".to_string(),
            lifecycle: LifecycleStatus::Reviewable,
            annotations: Vec::new(),
            primary_action: OperatorAction::Resume,
            available_actions: vec![
                OperatorAction::Start,
                OperatorAction::Resume,
                OperatorAction::Review,
                OperatorAction::Ship,
            ],
            live_summary: Some("waiting for review".to_string()),
        };

        let browser = browser_task_card(&card);

        assert_eq!(browser.qualified_handle, "web/fix-login");
        assert_eq!(browser.ui_state, "review ready");
        assert_eq!(browser.status_label, "review ready");
        assert_eq!(browser.lifecycle, "Reviewable");
        assert_eq!(browser.live_summary.as_deref(), Some("waiting for review"));
        assert_eq!(browser.primary_action, "review");
        assert_eq!(browser.available_actions, ["review", "ship"]);
    }
}
