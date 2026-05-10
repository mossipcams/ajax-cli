use ajax_core::models::RecommendedAction;

/// Per-task action menu contents, ordered with the most likely action first.
pub(crate) fn task_action_list(is_review: bool) -> Vec<&'static str> {
    if is_review {
        vec![
            RecommendedAction::ReviewBranch.as_str(),
            RecommendedAction::OpenTask.as_str(),
            RecommendedAction::DiffTask.as_str(),
            RecommendedAction::CheckTask.as_str(),
            RecommendedAction::MergeTask.as_str(),
            RecommendedAction::OpenWorktrunk.as_str(),
            RecommendedAction::InspectTask.as_str(),
            RecommendedAction::CleanTask.as_str(),
        ]
    } else {
        vec![
            RecommendedAction::OpenTask.as_str(),
            RecommendedAction::DiffTask.as_str(),
            RecommendedAction::CheckTask.as_str(),
            RecommendedAction::MergeTask.as_str(),
            RecommendedAction::ReviewBranch.as_str(),
            RecommendedAction::OpenWorktrunk.as_str(),
            RecommendedAction::InspectTask.as_str(),
            RecommendedAction::CleanTask.as_str(),
        ]
    }
}
