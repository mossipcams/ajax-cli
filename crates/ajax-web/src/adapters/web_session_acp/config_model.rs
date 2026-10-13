//! Neutral model of a session config option as the web adapter uses it.
//!
//! This mirrors [`agent_client_protocol::schema::v1::SessionConfigOption`] without
//! depending on it, so native (non-ACP) adapters such as the Claude SDK and Pi RPC
//! clients can describe their config options in one shared shape. ACP edges convert
//! through the `From` impls below; nothing here is protocol-specific.

use agent_client_protocol::schema::v1::{
    SessionConfigBoolean, SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigOptionValue, SessionConfigSelect, SessionConfigSelectOption,
    SessionConfigSelectOptions,
};

/// Neutral value for a session config set operation. Mirrors what
/// `SessionConfigOptionValue` carries today: a string value id for select-style
/// options or a boolean toggle value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeutralConfigValue {
    /// A string value id (select-style option).
    ValueId(String),
    /// A boolean toggle value.
    Boolean(bool),
}

impl NeutralConfigValue {
    pub fn value_id(value: impl Into<String>) -> Self {
        Self::ValueId(value.into())
    }

    pub fn boolean(value: bool) -> Self {
        Self::Boolean(value)
    }
}

/// Neutral semantic category for a session config option (UX only). The web adapter
/// treats everything that is neither `Model` nor `ThoughtLevel` as an opaque name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeutralConfigCategory {
    /// Model selector.
    Model,
    /// Thought/reasoning level selector.
    ThoughtLevel,
    /// Any other known or custom category name (`mode`, `model_config`, `_custom`, ...).
    Other(String),
}

/// One selectable choice of a select-style config option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeutralConfigChoice {
    /// Value id to send back when the user picks this choice.
    pub value: String,
    /// Human-readable label for the choice.
    pub name: String,
    /// Optional description for the user.
    pub description: Option<String>,
}

/// Type-specific payload of a session config option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeutralConfigKind {
    /// Single-value selector (dropdown) with its current value and choices.
    Select {
        current_value: String,
        choices: Vec<NeutralConfigChoice>,
    },
    /// Boolean on/off toggle with its current value.
    Boolean { current_value: bool },
    /// Payload kind the neutral model does not represent (future ACP kinds).
    /// The option stays visible by id/name/category but carries no state;
    /// converting it back to ACP yields an empty select placeholder.
    Unsupported,
}

/// A neutral session config option: id, name, optional description, optional
/// category, and the type-specific current state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeutralConfigOption {
    /// Unique identifier for the configuration option.
    pub id: String,
    /// Human-readable label for the option.
    pub name: String,
    /// Optional description for the user.
    pub description: Option<String>,
    /// Optional semantic category (UX only).
    pub category: Option<NeutralConfigCategory>,
    /// Type-specific current state of the option.
    pub kind: NeutralConfigKind,
}

fn choice_from(option: &SessionConfigSelectOption) -> NeutralConfigChoice {
    NeutralConfigChoice {
        value: option.value.0.to_string(),
        name: option.name.clone(),
        description: option.description.clone(),
    }
}

fn select_choices(select: &SessionConfigSelect) -> Vec<NeutralConfigChoice> {
    match &select.options {
        SessionConfigSelectOptions::Ungrouped(items) => items.iter().map(choice_from).collect(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter())
            .map(choice_from)
            .collect(),
        // `SessionConfigSelectOptions` is `#[non_exhaustive]`; a future layout
        // contributes no choices rather than failing the whole conversion.
        _ => Vec::new(),
    }
}

impl From<&SessionConfigOption> for NeutralConfigOption {
    fn from(option: &SessionConfigOption) -> Self {
        let kind = match &option.kind {
            SessionConfigKind::Select(select) => NeutralConfigKind::Select {
                current_value: select.current_value.0.to_string(),
                choices: select_choices(select),
            },
            SessionConfigKind::Boolean(boolean) => NeutralConfigKind::Boolean {
                current_value: boolean.current_value,
            },
            // `SessionConfigKind` is `#[non_exhaustive]`; unknown payload kinds
            // degrade to the neutral `Unsupported` kind.
            _ => NeutralConfigKind::Unsupported,
        };
        Self {
            id: option.id.0.to_string(),
            name: option.name.clone(),
            description: option.description.clone(),
            category: option.category.as_ref().map(|category| category.into()),
            kind,
        }
    }
}

impl From<&NeutralConfigOption> for SessionConfigOption {
    fn from(option: &NeutralConfigOption) -> Self {
        let kind = match &option.kind {
            NeutralConfigKind::Select {
                current_value,
                choices,
            } => {
                let options: Vec<SessionConfigSelectOption> = choices
                    .iter()
                    .map(|choice| {
                        SessionConfigSelectOption::new(choice.value.clone(), choice.name.clone())
                            .description(choice.description.clone())
                    })
                    .collect();
                SessionConfigKind::Select(SessionConfigSelect::new(current_value.clone(), options))
            }
            NeutralConfigKind::Boolean { current_value } => {
                SessionConfigKind::Boolean(SessionConfigBoolean::new(*current_value))
            }
            // No ACP counterpart exists; this is the documented lossy placeholder.
            NeutralConfigKind::Unsupported => SessionConfigKind::Select(SessionConfigSelect::new(
                String::new(),
                SessionConfigSelectOptions::Ungrouped(Vec::new()),
            )),
        };
        let mut built = SessionConfigOption::new(option.id.clone(), option.name.clone(), kind);
        built.description = option.description.clone();
        built.category = option.category.as_ref().map(|category| category.into());
        built
    }
}

impl From<&SessionConfigOptionCategory> for NeutralConfigCategory {
    fn from(category: &SessionConfigOptionCategory) -> Self {
        match category {
            SessionConfigOptionCategory::Model => Self::Model,
            SessionConfigOptionCategory::ThoughtLevel => Self::ThoughtLevel,
            // `SessionConfigOptionCategory` has no Display impl; map named
            // categories to their wire names so the reverse conversion can find
            // them again (see the neutral -> ACP mapping below).
            SessionConfigOptionCategory::Mode => Self::Other("mode".to_string()),
            SessionConfigOptionCategory::ModelConfig => Self::Other("model_config".to_string()),
            SessionConfigOptionCategory::Other(name) => Self::Other(name.clone()),
            // The ACP category enum is `#[non_exhaustive]`; future reserved
            // categories degrade to an opaque `_unknown` name.
            _ => Self::Other("_unknown".to_string()),
        }
    }
}

impl From<&NeutralConfigCategory> for SessionConfigOptionCategory {
    fn from(category: &NeutralConfigCategory) -> Self {
        match category {
            NeutralConfigCategory::Model => Self::Model,
            NeutralConfigCategory::ThoughtLevel => Self::ThoughtLevel,
            NeutralConfigCategory::Other(name) => {
                if name == "mode" {
                    Self::Mode
                } else if name == "model_config" {
                    Self::ModelConfig
                } else {
                    Self::Other(name.clone())
                }
            }
        }
    }
}

impl From<&SessionConfigOptionValue> for NeutralConfigValue {
    fn from(value: &SessionConfigOptionValue) -> Self {
        match value {
            SessionConfigOptionValue::Boolean { value } => Self::Boolean(*value),
            SessionConfigOptionValue::ValueId { value } => Self::ValueId(value.0.to_string()),
            // `SessionConfigOptionValue` is `#[non_exhaustive]`; unknown value
            // shapes degrade to an empty value id.
            _ => Self::ValueId(String::new()),
        }
    }
}

impl From<NeutralConfigValue> for SessionConfigOptionValue {
    fn from(value: NeutralConfigValue) -> Self {
        match value {
            NeutralConfigValue::Boolean(value) => Self::boolean(value),
            NeutralConfigValue::ValueId(id) => Self::value_id(id),
        }
    }
}
