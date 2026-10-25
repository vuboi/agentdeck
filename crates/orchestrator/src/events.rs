use serde::{Deserialize, Serialize};

use crate::model::AttemptStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowEvent {
    RunStarted {
        at: i64,
    },
    AttemptStarted {
        step_id: String,
        attempt_id: String,
        session_id: String,
        at: i64,
    },
    AttemptWaitingInput {
        step_id: String,
        attempt_id: String,
        at: i64,
    },
    AttemptResumed {
        step_id: String,
        attempt_id: String,
        at: i64,
    },
    AttemptFinished {
        step_id: String,
        attempt_id: String,
        status: AttemptStatus,
        at: i64,
        detail: Option<String>,
    },
    RunCancelled {
        at: i64,
    },
}
