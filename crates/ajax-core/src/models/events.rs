use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::intent::TaskId;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
pub enum TaskOperationKind {
    Start,
    Ship,
    Drop,
    Repair,
    Tidy,
}

impl TaskOperationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Ship => "ship",
            Self::Drop => "drop",
            Self::Repair => "repair",
            Self::Tidy => "tidy",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "start" => Some(Self::Start),
            "ship" => Some(Self::Ship),
            "drop" => Some(Self::Drop),
            "repair" => Some(Self::Repair),
            "tidy" => Some(Self::Tidy),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
pub enum StepReceiptStatus {
    Succeeded,
    Failed,
    SkippedObserved,
}

impl StepReceiptStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::SkippedObserved => "skipped_observed",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "skipped_observed" => Some(Self::SkippedObserved),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct StepReceiptIdentity {
    pub task_id: TaskId,
    pub operation: TaskOperationKind,
    pub step_key: String,
    pub target: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct StepReceipt {
    pub task_id: TaskId,
    pub operation: TaskOperationKind,
    pub step_key: String,
    pub target: String,
    pub status: StepReceiptStatus,
    pub receipt_json: String,
    pub created_at: SystemTime,
}

impl StepReceipt {
    pub fn new(
        task_id: TaskId,
        operation: TaskOperationKind,
        step_key: impl Into<String>,
        target: impl Into<String>,
        status: StepReceiptStatus,
        receipt_json: impl Into<String>,
    ) -> Self {
        Self {
            task_id,
            operation,
            step_key: step_key.into(),
            target: target.into(),
            status,
            receipt_json: receipt_json.into(),
            created_at: SystemTime::now(),
        }
    }

    pub fn succeeded(
        task_id: TaskId,
        operation: TaskOperationKind,
        step_key: impl Into<String>,
        target: impl Into<String>,
        receipt_json: impl Into<String>,
    ) -> Self {
        Self::new(
            task_id,
            operation,
            step_key,
            target,
            StepReceiptStatus::Succeeded,
            receipt_json,
        )
    }

    pub fn identity(&self) -> StepReceiptIdentity {
        StepReceiptIdentity {
            task_id: self.task_id.clone(),
            operation: self.operation,
            step_key: self.step_key.clone(),
            target: self.target.clone(),
        }
    }
}
