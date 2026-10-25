use thiserror::Error;

use crate::model::RunStatus;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TransitionError {
    #[error("run is already {0:?}")]
    RunNotMutable(RunStatus),
    #[error("step not found: {0}")]
    StepNotFound(String),
    #[error("step {step_id} is not ready")]
    StepNotReady { step_id: String },
    #[error("step {step_id} is already running")]
    StepAlreadyRunning { step_id: String },
    #[error("attempt not found: {attempt_id}")]
    AttemptNotFound { attempt_id: String },
    #[error("attempt {attempt_id} is not active")]
    AttemptNotActive { attempt_id: String },
    #[error("step {step_id} has no retry left")]
    RetryLimitReached { step_id: String },
    #[error("duplicate step: {0}")]
    DuplicateStep(String),
    #[error("step {step_id} depends on missing step {dependency}")]
    MissingDependency { step_id: String, dependency: String },
    #[error("step {step_id} cannot depend on itself")]
    SelfDependency { step_id: String },
}
