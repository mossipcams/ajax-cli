//! Session attach plan passed from the slice to the Cursor ACP adapter.

use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionAttachPlan {
    pub qualified_handle: String,
    pub worktree: PathBuf,
}
