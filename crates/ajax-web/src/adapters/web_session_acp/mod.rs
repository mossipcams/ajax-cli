//! Cursor ACP stdio host for Ajax Web Session orchestration chat.

mod client;

#[cfg(test)]
mod client_tests;

#[cfg(test)]
mod spawn_tests;

pub use client::{AcpClientEvent, AcpStdioClient, SpawnReport};

#[cfg(test)]
pub(crate) use client::{with_test_acp_extra_args, with_test_acp_program};
