use std::collections::BTreeSet;

use crate::{
    Attempt, AttemptStatus, Run, RunStatus, Step, StepSpec, StepStatus, TransitionError,
    WorkflowEvent,
};

impl Run {
    pub fn new(
        id: impl Into<String>,
        created_at: i64,
        specs: impl IntoIterator<Item = StepSpec>,
    ) -> Result<Self, TransitionError> {
        let specs: Vec<StepSpec> = specs.into_iter().collect();
        let ids: BTreeSet<String> = specs.iter().map(|spec| spec.id.clone()).collect();
        if let Some(duplicate) = specs.iter().find_map(|spec| {
            (specs.iter().filter(|other| other.id == spec.id).count() > 1).then(|| spec.id.clone())
        }) {
            return Err(TransitionError::DuplicateStep(duplicate));
        }
        for spec in &specs {
            for dependency in &spec.depends_on {
                if dependency == &spec.id {
                    return Err(TransitionError::SelfDependency {
                        step_id: spec.id.clone(),
                    });
                }
                if !ids.contains(dependency) {
                    return Err(TransitionError::MissingDependency {
                        step_id: spec.id.clone(),
                        dependency: dependency.clone(),
                    });
                }
            }
        }
        let steps = specs
            .into_iter()
            .map(|spec| {
                (
                    spec.id.clone(),
                    Step {
                        spec,
                        status: StepStatus::Pending,
                        attempts: Vec::new(),
                    },
                )
            })
            .collect();
        Ok(Self {
            id: id.into(),
            status: RunStatus::Pending,
            created_at,
            started_at: None,
            ended_at: None,
            steps,
            events: Vec::new(),
        })
    }

    pub fn events(&self) -> &[WorkflowEvent] {
        &self.events
    }

    pub fn ready_steps(&self) -> Vec<&str> {
        self.steps
            .values()
            .filter(|step| {
                step.status == StepStatus::Pending
                    && step.spec.depends_on.iter().all(|dependency| {
                        self.steps
                            .get(dependency)
                            .is_some_and(|item| item.status == StepStatus::Succeeded)
                    })
            })
            .map(|step| step.spec.id.as_str())
            .collect()
    }

    pub fn start(&mut self, at: i64) -> Result<(), TransitionError> {
        if self.status != RunStatus::Pending {
            return Err(TransitionError::RunNotMutable(self.status));
        }
        self.status = RunStatus::Running;
        self.started_at = Some(at);
        self.events.push(WorkflowEvent::RunStarted { at });
        Ok(())
    }

    pub fn begin_attempt(
        &mut self,
        step_id: &str,
        attempt_id: impl Into<String>,
        session_id: impl Into<String>,
        at: i64,
    ) -> Result<(), TransitionError> {
        self.ensure_running()?;
        let dependencies_succeeded = self.dependencies_succeeded(step_id)?;
        let (step_status, attempts, max_attempts) = {
            let step = self
                .steps
                .get(step_id)
                .ok_or_else(|| TransitionError::StepNotFound(step_id.to_owned()))?;
            (step.status, step.attempts.len(), step.spec.max_attempts)
        };
        if step_status == StepStatus::Running || step_status == StepStatus::WaitingInput {
            return Err(TransitionError::StepAlreadyRunning {
                step_id: step_id.to_owned(),
            });
        }
        if !dependencies_succeeded {
            return Err(TransitionError::StepNotReady {
                step_id: step_id.to_owned(),
            });
        }
        if attempts >= max_attempts as usize {
            return Err(TransitionError::RetryLimitReached {
                step_id: step_id.to_owned(),
            });
        }
        let attempt_id = attempt_id.into();
        let session_id = session_id.into();
        let step = self.step_mut(step_id)?;
        step.status = StepStatus::Running;
        step.attempts.push(Attempt {
            id: attempt_id.clone(),
            session_id: session_id.clone(),
            status: AttemptStatus::Running,
            started_at: at,
            ended_at: None,
            detail: None,
        });
        self.events.push(WorkflowEvent::AttemptStarted {
            step_id: step_id.to_owned(),
            attempt_id,
            session_id,
            at,
        });
        Ok(())
    }

    pub fn wait_for_input(
        &mut self,
        step_id: &str,
        attempt_id: &str,
        at: i64,
    ) -> Result<(), TransitionError> {
        self.ensure_attempt_active(step_id, attempt_id)?;
        let step = self.step_mut(step_id)?;
        let attempt = step
            .attempts
            .last_mut()
            .ok_or_else(|| TransitionError::AttemptNotFound {
                attempt_id: attempt_id.to_owned(),
            })?;
        attempt.status = AttemptStatus::WaitingInput;
        step.status = StepStatus::WaitingInput;
        self.status = RunStatus::WaitingInput;
        self.events.push(WorkflowEvent::AttemptWaitingInput {
            step_id: step_id.to_owned(),
            attempt_id: attempt_id.to_owned(),
            at,
        });
        Ok(())
    }

    pub fn resume_attempt(
        &mut self,
        step_id: &str,
        attempt_id: &str,
        at: i64,
    ) -> Result<(), TransitionError> {
        let step = self.step_mut(step_id)?;
        let attempt = step
            .attempts
            .last_mut()
            .ok_or_else(|| TransitionError::AttemptNotFound {
                attempt_id: attempt_id.to_owned(),
            })?;
        if attempt.id != attempt_id || attempt.status != AttemptStatus::WaitingInput {
            return Err(TransitionError::AttemptNotActive {
                attempt_id: attempt_id.to_owned(),
            });
        }
        attempt.status = AttemptStatus::Running;
        step.status = StepStatus::Running;
        self.status = RunStatus::Running;
        self.events.push(WorkflowEvent::AttemptResumed {
            step_id: step_id.to_owned(),
            attempt_id: attempt_id.to_owned(),
            at,
        });
        Ok(())
    }

    pub fn finish_attempt(
        &mut self,
        step_id: &str,
        attempt_id: &str,
        success: bool,
        at: i64,
        detail: Option<String>,
    ) -> Result<(), TransitionError> {
        self.ensure_attempt_active(step_id, attempt_id)?;
        let attempt_status = if success {
            AttemptStatus::Succeeded
        } else {
            AttemptStatus::Failed
        };
        {
            let step = self.step_mut(step_id)?;
            let attempt_count = step.attempts.len();
            let max_attempts = step.spec.max_attempts as usize;
            let attempt =
                step.attempts
                    .last_mut()
                    .ok_or_else(|| TransitionError::AttemptNotFound {
                        attempt_id: attempt_id.to_owned(),
                    })?;
            attempt.status = attempt_status;
            attempt.ended_at = Some(at);
            attempt.detail = detail.clone();
            step.status = if success {
                StepStatus::Succeeded
            } else if attempt_count < max_attempts {
                StepStatus::Pending
            } else {
                StepStatus::Failed
            };
        }
        self.events.push(WorkflowEvent::AttemptFinished {
            step_id: step_id.to_owned(),
            attempt_id: attempt_id.to_owned(),
            status: attempt_status,
            at,
            detail,
        });
        self.recompute_status(at);
        Ok(())
    }

    pub fn cancel(&mut self, at: i64) -> Result<(), TransitionError> {
        self.ensure_not_terminal()?;
        for step in self.steps.values_mut() {
            if matches!(
                step.status,
                StepStatus::Pending | StepStatus::Running | StepStatus::WaitingInput
            ) {
                step.status = StepStatus::Cancelled;
                if let Some(attempt) = step.attempts.last_mut().filter(|attempt| {
                    matches!(
                        attempt.status,
                        AttemptStatus::Running | AttemptStatus::WaitingInput
                    )
                }) {
                    attempt.status = AttemptStatus::Cancelled;
                    attempt.ended_at = Some(at);
                }
            }
        }
        self.status = RunStatus::Cancelled;
        self.ended_at = Some(at);
        self.events.push(WorkflowEvent::RunCancelled { at });
        Ok(())
    }

    fn step_mut(&mut self, step_id: &str) -> Result<&mut Step, TransitionError> {
        self.steps
            .get_mut(step_id)
            .ok_or_else(|| TransitionError::StepNotFound(step_id.to_owned()))
    }

    fn dependencies_succeeded(&self, step_id: &str) -> Result<bool, TransitionError> {
        let step = self
            .steps
            .get(step_id)
            .ok_or_else(|| TransitionError::StepNotFound(step_id.to_owned()))?;
        Ok(step.spec.depends_on.iter().all(|dependency| {
            self.steps
                .get(dependency)
                .is_some_and(|item| item.status == StepStatus::Succeeded)
        }))
    }

    fn ensure_running(&self) -> Result<(), TransitionError> {
        if self.status == RunStatus::Running {
            Ok(())
        } else {
            Err(TransitionError::RunNotMutable(self.status))
        }
    }

    fn ensure_not_terminal(&self) -> Result<(), TransitionError> {
        if matches!(
            self.status,
            RunStatus::Succeeded | RunStatus::Failed | RunStatus::Cancelled
        ) {
            Err(TransitionError::RunNotMutable(self.status))
        } else {
            Ok(())
        }
    }

    fn ensure_attempt_active(
        &self,
        step_id: &str,
        attempt_id: &str,
    ) -> Result<(), TransitionError> {
        let step = self
            .steps
            .get(step_id)
            .ok_or_else(|| TransitionError::StepNotFound(step_id.to_owned()))?;
        let attempt = step
            .attempts
            .last()
            .ok_or_else(|| TransitionError::AttemptNotFound {
                attempt_id: attempt_id.to_owned(),
            })?;
        if attempt.id != attempt_id
            || !matches!(
                attempt.status,
                AttemptStatus::Running | AttemptStatus::WaitingInput
            )
        {
            return Err(TransitionError::AttemptNotActive {
                attempt_id: attempt_id.to_owned(),
            });
        }
        Ok(())
    }

    fn recompute_status(&mut self, at: i64) {
        if self
            .steps
            .values()
            .any(|step| step.status == StepStatus::Failed)
        {
            self.status = RunStatus::Failed;
        } else if self
            .steps
            .values()
            .all(|step| matches!(step.status, StepStatus::Succeeded | StepStatus::Skipped))
        {
            self.status = RunStatus::Succeeded;
        } else if self
            .steps
            .values()
            .any(|step| step.status == StepStatus::WaitingInput)
        {
            self.status = RunStatus::WaitingInput;
        } else {
            self.status = RunStatus::Running;
        }
        if matches!(self.status, RunStatus::Succeeded | RunStatus::Failed) {
            self.ended_at = Some(at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run() -> Run {
        Run::new(
            "run-1",
            1,
            [
                StepSpec::new("plan").retry_up_to(2),
                StepSpec::new("implement").after("plan"),
            ],
        )
        .expect("valid graph")
    }

    #[test]
    fn dependencies_gate_ready_steps() {
        let mut run = run();
        assert_eq!(run.ready_steps(), vec!["plan"]);
        run.start(2).unwrap();
        run.begin_attempt("plan", "a1", "s1", 3).unwrap();
        assert!(run.ready_steps().is_empty());
        run.finish_attempt("plan", "a1", true, 4, Some("planned".into()))
            .unwrap();
        assert_eq!(run.ready_steps(), vec!["implement"]);
    }

    #[test]
    fn failed_attempt_can_retry_in_the_same_run() {
        let mut run = run();
        run.start(2).unwrap();
        run.begin_attempt("plan", "a1", "s1", 3).unwrap();
        run.finish_attempt("plan", "a1", false, 4, Some("bad input".into()))
            .unwrap();
        assert_eq!(run.status, RunStatus::Running);
        assert_eq!(run.steps["plan"].status, StepStatus::Pending);
        run.begin_attempt("plan", "a2", "s1", 5).unwrap();
        assert_eq!(run.steps["plan"].attempts[1].session_id, "s1");
    }

    #[test]
    fn waiting_input_pauses_and_resume_continues_attempt() {
        let mut run = run();
        run.start(2).unwrap();
        run.begin_attempt("plan", "a1", "s1", 3).unwrap();
        run.wait_for_input("plan", "a1", 4).unwrap();
        assert_eq!(run.status, RunStatus::WaitingInput);
        run.resume_attempt("plan", "a1", 5).unwrap();
        assert_eq!(run.status, RunStatus::Running);
        assert_eq!(run.steps["plan"].attempts[0].status, AttemptStatus::Running);
    }

    #[test]
    fn terminal_success_and_cancel_are_recorded() {
        let mut active = run();
        active.start(2).unwrap();
        active.begin_attempt("plan", "a1", "s1", 3).unwrap();
        active.finish_attempt("plan", "a1", true, 4, None).unwrap();
        active.begin_attempt("implement", "a2", "s2", 5).unwrap();
        active
            .finish_attempt("implement", "a2", true, 6, None)
            .unwrap();
        assert_eq!(active.status, RunStatus::Succeeded);
        assert_eq!(active.ended_at, Some(6));

        let mut cancelled = run();
        cancelled.start(2).unwrap();
        cancelled.cancel(3).unwrap();
        assert_eq!(cancelled.status, RunStatus::Cancelled);
        assert!(cancelled
            .steps
            .values()
            .all(|step| step.status == StepStatus::Cancelled));
    }

    #[test]
    fn invalid_graph_is_rejected() {
        let error = Run::new("run", 0, [StepSpec::new("a").after("missing")]).unwrap_err();
        assert!(matches!(error, TransitionError::MissingDependency { .. }));
    }
}
