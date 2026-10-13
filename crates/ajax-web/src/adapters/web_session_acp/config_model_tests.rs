//! Round-trip tests for the neutral config model: ACP -> neutral -> ACP must
//! reproduce every shape `config_options.rs` handles today (select, grouped
//! select, boolean, categories, descriptions, values).

use super::config_model::{
    NeutralConfigCategory, NeutralConfigKind, NeutralConfigOption, NeutralConfigValue,
};
use agent_client_protocol::schema::v1::{
    SessionConfigBoolean, SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigOptionValue, SessionConfigSelect, SessionConfigSelectGroup,
    SessionConfigSelectOption, SessionConfigSelectOptions,
};

fn choice(value: &str, name: &str, description: Option<&str>) -> SessionConfigSelectOption {
    let mut option = SessionConfigSelectOption::new(value.to_string(), name.to_string());
    if let Some(description) = description {
        option.description = Some(description.to_string());
    }
    option
}

fn select_option(
    id: &str,
    current: &str,
    options: Vec<SessionConfigSelectOption>,
) -> SessionConfigOption {
    SessionConfigOption::new(
        id.to_string(),
        format!("name-{id}"),
        SessionConfigKind::Select(SessionConfigSelect::new(current.to_string(), options)),
    )
}

#[test]
fn select_round_trips_with_descriptions_and_category() {
    let acp = select_option(
        "model",
        "opus",
        vec![
            choice("opus", "Claude Opus 4.8", Some("Fast and intelligent.")),
            choice("sonnet", "Claude Sonnet 4.5", None),
            choice("haiku", "Claude Haiku 4.5", Some("Quick drafts.")),
        ],
    );

    let neutral: NeutralConfigOption = (&acp).into();
    assert_eq!(neutral.id, "model");
    match &neutral.kind {
        NeutralConfigKind::Select {
            current_value,
            choices,
        } => {
            assert_eq!(current_value, "opus");
            assert_eq!(choices.len(), 3);
            assert_eq!(choices[0].value, "opus");
            assert_eq!(
                choices[0].description.as_deref(),
                Some("Fast and intelligent.")
            );
            assert_eq!(choices[1].description, None);
        }
        other => panic!("expected select kind, got {other:?}"),
    }

    let back: SessionConfigOption = (&neutral).into();
    assert_eq!(back, acp);

    // Description and category survive the round trip too.
    let with_meta = SessionConfigOption::new(
        "model".to_string(),
        "Model".to_string(),
        match acp.kind {
            SessionConfigKind::Select(select) => SessionConfigKind::Select(select),
            other => panic!("expected select kind, got {other:?}"),
        },
    )
    .description(Some("Pick a model".to_string()))
    .category(SessionConfigOptionCategory::Model);

    let neutral = NeutralConfigOption::from(&with_meta);
    assert_eq!(neutral.name, "Model");
    assert_eq!(neutral.description.as_deref(), Some("Pick a model"));
    assert_eq!(neutral.category, Some(NeutralConfigCategory::Model));

    let back: SessionConfigOption = (&neutral).into();
    assert_eq!(back, with_meta);
}

#[test]
fn grouped_select_flattens_to_ungrouped_and_round_trips() {
    let groups = vec![
        SessionConfigSelectGroup::new(
            "anthropic".to_string(),
            "Anthropic".to_string(),
            vec![
                choice("opus", "Claude Opus 4.8", None),
                choice("haiku", "Claude Haiku 4.5", None),
            ],
        ),
        SessionConfigSelectGroup::new(
            "openai".to_string(),
            "OpenAI".to_string(),
            vec![choice("gpt", "GPT-5.6", None)],
        ),
    ];
    let acp = SessionConfigOption::new(
        "model".to_string(),
        "Model".to_string(),
        SessionConfigKind::Select(SessionConfigSelect::new(
            "opus".to_string(),
            SessionConfigSelectOptions::Grouped(groups),
        )),
    );

    let neutral: NeutralConfigOption = (&acp).into();
    match &neutral.kind {
        // Groups are flattened in group order.
        NeutralConfigKind::Select { choices, .. } => assert_eq!(
            choices.iter().map(|c| c.value.as_str()).collect::<Vec<_>>(),
            vec!["opus", "haiku", "gpt"]
        ),
        other => panic!("expected select kind, got {other:?}"),
    }

    // neutral -> ACP is ungrouped with the flattened choice list; equality is
    // against that canonical form.
    let back: SessionConfigOption = (&neutral).into();
    // (The `select_option` helper names its option `name-{id}`, so the
    // expected value is built directly to keep the round-tripped name.)
    let expected = SessionConfigOption::new(
        "model".to_string(),
        "Model".to_string(),
        SessionConfigKind::Select(SessionConfigSelect::new(
            "opus".to_string(),
            vec![
                choice("opus", "Claude Opus 4.8", None),
                choice("haiku", "Claude Haiku 4.5", None),
                choice("gpt", "GPT-5.6", None),
            ],
        )),
    );
    assert_eq!(back, expected);
}

#[test]
fn boolean_round_trips() {
    let acp = SessionConfigOption::new(
        "fast".to_string(),
        "Fast mode".to_string(),
        SessionConfigKind::Boolean(SessionConfigBoolean::new(true)),
    )
    .category(SessionConfigOptionCategory::ModelConfig);

    let neutral: NeutralConfigOption = (&acp).into();
    match &neutral.kind {
        NeutralConfigKind::Boolean { current_value } => assert!(*current_value),
        other => panic!("expected boolean kind, got {other:?}"),
    }

    let back: SessionConfigOption = (&neutral).into();
    assert_eq!(back, acp);

    // False flips back identically.
    let off = NeutralConfigOption {
        id: "fast".to_string(),
        name: "Fast mode".to_string(),
        description: None,
        category: Some(NeutralConfigCategory::Other("model_config".to_string())),
        kind: NeutralConfigKind::Boolean {
            current_value: false,
        },
    };
    let acp_off = SessionConfigOption::new(
        "fast".to_string(),
        "Fast mode".to_string(),
        SessionConfigKind::Boolean(SessionConfigBoolean::new(false)),
    )
    .category(SessionConfigOptionCategory::ModelConfig);
    assert_eq!(SessionConfigOption::from(&off), acp_off);
}

#[test]
fn categories_map_both_directions() {
    let cases: Vec<(SessionConfigOptionCategory, NeutralConfigCategory)> = vec![
        (
            SessionConfigOptionCategory::Mode,
            NeutralConfigCategory::Other("mode".to_string()),
        ),
        (
            SessionConfigOptionCategory::Model,
            NeutralConfigCategory::Model,
        ),
        (
            SessionConfigOptionCategory::ModelConfig,
            NeutralConfigCategory::Other("model_config".to_string()),
        ),
        (
            SessionConfigOptionCategory::ThoughtLevel,
            NeutralConfigCategory::ThoughtLevel,
        ),
        (
            SessionConfigOptionCategory::Other("_custom".to_string()),
            NeutralConfigCategory::Other("_custom".to_string()),
        ),
    ];

    for (acp_category, neutral_expected) in cases {
        let neutral: NeutralConfigCategory = (&acp_category).into();
        assert_eq!(neutral, neutral_expected);
        let back: SessionConfigOptionCategory = (&neutral).into();
        assert_eq!(back, acp_category);
    }
}

#[test]
fn absent_category_and_description_round_trip() {
    let acp = select_option(
        "thought_level",
        "high",
        vec![choice("low", "Low", None), choice("high", "High", None)],
    );

    let neutral: NeutralConfigOption = (&acp).into();
    assert_eq!(neutral.category, None);
    assert_eq!(neutral.description, None);

    let back: SessionConfigOption = (&neutral).into();
    assert_eq!(back, acp);
}

#[test]
fn value_types_round_trip_both_directions() {
    // ACP -> neutral.
    assert_eq!(
        NeutralConfigValue::from(&SessionConfigOptionValue::value_id("opus".to_string())),
        NeutralConfigValue::ValueId("opus".to_string())
    );
    assert_eq!(
        NeutralConfigValue::from(&SessionConfigOptionValue::boolean(true)),
        NeutralConfigValue::Boolean(true)
    );

    // neutral -> ACP.
    assert_eq!(
        SessionConfigOptionValue::from(NeutralConfigValue::value_id("opus")),
        SessionConfigOptionValue::value_id("opus".to_string())
    );
    assert_eq!(
        SessionConfigOptionValue::from(NeutralConfigValue::boolean(false)),
        SessionConfigOptionValue::boolean(false)
    );

    // Full both-direction round trip on an option carrying a select current value.
    let acp = select_option("model", "sonnet", vec![choice("sonnet", "Sonnet", None)]);
    let neutral: NeutralConfigOption = (&acp).into();
    let back: SessionConfigOption = (&neutral).into();
    assert_eq!(back, acp);
}

#[test]
fn unsupported_kind_uses_documented_empty_select_placeholder() {
    // `NeutralConfigKind::Unsupported` models ACP payload kinds the neutral
    // model cannot represent. The only constructible check is on the way back
    // to ACP: it degrades lossily to an empty select placeholder while keeping
    // id/name/category intact.
    let neutral = NeutralConfigOption {
        id: "future".to_string(),
        name: "Future kind".to_string(),
        description: Some("Unknown payload".to_string()),
        category: Some(NeutralConfigCategory::Other("_custom".to_string())),
        kind: NeutralConfigKind::Unsupported,
    };

    let acp = SessionConfigOption::from(&neutral);
    assert_eq!(&*acp.id.0, "future");
    assert_eq!(acp.name, "Future kind");
    assert_eq!(acp.description.as_deref(), Some("Unknown payload"));
    assert_eq!(
        acp.category,
        Some(SessionConfigOptionCategory::Other("_custom".to_string()))
    );
    assert_eq!(
        acp.kind,
        SessionConfigKind::Select(SessionConfigSelect::new(
            String::new(),
            SessionConfigSelectOptions::Ungrouped(Vec::new()),
        ))
    );

    // The lossy placeholder reads back as a regular (empty) select, not as
    // `Unsupported` again.
    let neutral_back: NeutralConfigOption = (&acp).into();
    match neutral_back.kind {
        NeutralConfigKind::Select {
            current_value,
            choices,
        } => {
            assert_eq!(current_value, "");
            assert!(choices.is_empty());
        }
        other => panic!("expected empty select kind, got {other:?}"),
    }
}
