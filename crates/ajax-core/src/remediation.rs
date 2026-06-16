//! Compatibility wrappers for skill-backed remediations.

use crate::{
    commands::CommandContext,
    models::Task,
    recommended::RemediationId,
    registry::Registry,
    slices::remediate::{self, RemediationError},
};

pub const FIX_CI: &str = "fix-ci";
pub const RESOLVE_MERGE_CONFLICTS: &str = "resolve-merge-conflicts";
pub use crate::slices::remediate::{RemediationOption, RemediationOutcome};

pub fn remediations_for_task(task: &Task) -> Vec<RemediationOption> {
    remediate::remediations_for_task(task)
}

pub fn is_remediation_action(action: &str) -> bool {
    remediate::is_remediation_action(action)
}

pub fn format_brief(remediation_id: &str, task: &Task, skill_path: &str) -> Option<String> {
    RemediationId::from_label(remediation_id)
        .map(|id| remediate::format_brief(id, task, skill_path))
}

pub fn execute_remediation<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl crate::adapters::CommandRunner,
    task_handle: &str,
    remediation_id: &str,
    skill_path: &str,
) -> Result<RemediationOutcome, RemediationError> {
    let remediation_id = RemediationId::from_label(remediation_id)
        .ok_or_else(|| RemediationError::UnknownRemediation(remediation_id.to_string()))?;
    remediate::execute_remediation(context, runner, task_handle, remediation_id, skill_path)
}
