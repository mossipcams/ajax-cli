use agent_client_protocol::schema::v1::{
    SessionConfigBoolean, SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelect, SessionConfigSelectOption, SessionConfigSelectOptions,
};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeutralConfigCategory {
    Model,
    ThoughtLevel,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeutralConfigChoice {
    pub value: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeutralConfigKind {
    Select {
        current_value: String,
        choices: Vec<NeutralConfigChoice>,
    },
    Boolean {
        current_value: bool,
    },
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeutralConfigOption {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<NeutralConfigCategory>,
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
            SessionConfigOptionCategory::Mode => Self::Other("mode".to_string()),
            SessionConfigOptionCategory::ModelConfig => Self::Other("model_config".to_string()),
            SessionConfigOptionCategory::Other(name) => Self::Other(name.clone()),
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
pub fn neutral_options(options: &[SessionConfigOption]) -> Vec<NeutralConfigOption> {
    options.iter().map(NeutralConfigOption::from).collect()
}

pub fn acp_options(options: &[NeutralConfigOption]) -> Vec<SessionConfigOption> {
    options.iter().map(SessionConfigOption::from).collect()
}

pub fn descriptors_from_neutral(
    options: &[NeutralConfigOption],
) -> Vec<super::config_option_descriptors::ConfigOptionDescriptor> {
    use super::config_option_descriptors::{ConfigOptionChoiceDescriptor, ConfigOptionDescriptor};

    options
        .iter()
        .filter_map(|option| {
            let (kind, current_value, choices) = match &option.kind {
                NeutralConfigKind::Select {
                    current_value,
                    choices,
                } => (
                    "select".to_string(),
                    Value::String(current_value.clone()),
                    choices
                        .iter()
                        .map(|choice| ConfigOptionChoiceDescriptor {
                            value: choice.value.clone(),
                            name: choice.name.clone(),
                        })
                        .collect(),
                ),
                NeutralConfigKind::Boolean { current_value } => (
                    "boolean".to_string(),
                    Value::Bool(*current_value),
                    Vec::new(),
                ),
                NeutralConfigKind::Unsupported => return None,
            };
            Some(ConfigOptionDescriptor {
                id: option.id.clone(),
                category: option.category.as_ref().map(|category| match category {
                    NeutralConfigCategory::Model => "model".to_string(),
                    NeutralConfigCategory::ThoughtLevel => "thought_level".to_string(),
                    NeutralConfigCategory::Other(name) => name.clone(),
                }),
                name: option.name.clone(),
                kind,
                current_value,
                choices,
            })
        })
        .collect()
}

pub fn persisted_model_id(options: &[NeutralConfigOption]) -> Result<String, String> {
    super::config_options::applied_model_id_for_persist(&acp_options(options))
}

pub fn triggers_model_persist(options: &[NeutralConfigOption], config_id: &str) -> bool {
    super::config_options::option_triggers_model_persist(&acp_options(options), config_id)
}
