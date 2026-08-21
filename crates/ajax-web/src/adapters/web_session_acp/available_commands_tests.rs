//! Unit tests for live ACP slash-command descriptors.

use super::available_command_descriptors::{
    available_command_descriptors, AvailableCommandDescriptor,
};
use agent_client_protocol::schema::v1::{
    AvailableCommand, AvailableCommandInput, UnstructuredCommandInput,
};

#[test]
fn descriptors_map_name_description_and_input_hint() {
    let commands = vec![
        AvailableCommand::new("web", "Query the web").input(AvailableCommandInput::Unstructured(
            UnstructuredCommandInput::new("query"),
        )),
        AvailableCommand::new("help", "Show help"),
    ];
    assert_eq!(
        available_command_descriptors(&commands),
        vec![
            AvailableCommandDescriptor {
                name: "web".to_string(),
                description: "Query the web".to_string(),
                input_hint: Some("query".to_string()),
            },
            AvailableCommandDescriptor {
                name: "help".to_string(),
                description: "Show help".to_string(),
                input_hint: None,
            },
        ]
    );
}
