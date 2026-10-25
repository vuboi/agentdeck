//! Durable-workflow domain primitives for AgentDeck.
//!
//! This crate owns the run/step/attempt state machine. UI code may project its state, but it must not
//! decide whether a step is ready or whether a retry is legal. Persistence is intentionally kept at the
//! boundary: every transition produces an audit event that a daemon can append to the event store.

mod error;
mod events;
mod model;
mod state;

pub use error::TransitionError;
pub use events::WorkflowEvent;
pub use model::{Attempt, AttemptStatus, Run, RunStatus, Step, StepSpec, StepStatus};
