use std::{future::Future, sync::Arc};

use dashmap::DashMap;

use crate::{
    state::ProcessHealthState,
    workers::policy::{DegradationReason, WorkerCriticality, WorkerStatus},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerLifecycle {
    Starting,
    Running,
    Exited,
    Panicked,
    Cancelled,
}

#[derive(Clone, Default)]
pub struct WorkerSupervisor {
    lifecycle: Arc<DashMap<&'static str, WorkerLifecycle>>,
}

impl WorkerSupervisor {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn<F, Fut>(
        &self,
        worker: &'static str,
        criticality: WorkerCriticality,
        process_health: Arc<ProcessHealthState>,
        starter: F,
    ) -> tokio::task::JoinHandle<()>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.lifecycle.insert(worker, WorkerLifecycle::Starting);
        let lifecycle = Arc::clone(&self.lifecycle);

        tokio::spawn(async move {
            lifecycle.insert(worker, WorkerLifecycle::Running);

            match tokio::spawn(starter()).await {
                Ok(()) => {
                    lifecycle.insert(worker, WorkerLifecycle::Exited);
                    record_runtime_failure(
                        worker,
                        criticality,
                        &process_health,
                        WorkerLifecycle::Exited,
                    );
                }
                Err(join_error) if join_error.is_panic() => {
                    lifecycle.insert(worker, WorkerLifecycle::Panicked);
                    record_runtime_failure(
                        worker,
                        criticality,
                        &process_health,
                        WorkerLifecycle::Panicked,
                    );
                }
                Err(_) => {
                    lifecycle.insert(worker, WorkerLifecycle::Cancelled);
                    record_runtime_failure(
                        worker,
                        criticality,
                        &process_health,
                        WorkerLifecycle::Cancelled,
                    );
                }
            }
        })
    }

    #[must_use]
    pub fn lifecycle_of(&self, worker: &'static str) -> Option<WorkerLifecycle> {
        self.lifecycle.get(worker).map(|state| *state)
    }
}

fn record_runtime_failure(
    worker: &'static str,
    criticality: WorkerCriticality,
    process_health: &ProcessHealthState,
    observed_lifecycle: WorkerLifecycle,
) {
    let runtime_status: WorkerStatus = criticality.on_runtime_failure();

    match criticality {
        WorkerCriticality::NonCritical => {
            process_health.mark_degraded();
        }
        WorkerCriticality::Critical => {
            process_health.mark_runtime_failed();
        }
    }

    if observed_lifecycle == WorkerLifecycle::Panicked {
        tracing::error!(
            worker,
            ?criticality,
            ?runtime_status,
            reason = ?DegradationReason::PanicRecovered,
            ?observed_lifecycle,
            "supervisor observed worker panic"
        );
    } else {
        tracing::error!(
            worker,
            ?criticality,
            ?runtime_status,
            ?observed_lifecycle,
            "supervisor observed worker task completion"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::state::ProcessHealth;

    use super::*;

    #[tokio::test]
    async fn worker_supervisor_observes_panic_and_keeps_sibling_running() {
        let supervisor = WorkerSupervisor::new();
        let process_health = Arc::new(ProcessHealthState::new(ProcessHealth::Healthy));
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();

        let running_worker = supervisor.spawn(
            "healthy_worker",
            WorkerCriticality::NonCritical,
            Arc::clone(&process_health),
            move || async move {
                let _ = stop_rx.await;
            },
        );

        let panicking_worker = supervisor.spawn(
            "panicking_worker",
            WorkerCriticality::NonCritical,
            Arc::clone(&process_health),
            || async {
                panic!("synthetic worker panic");
            },
        );

        panicking_worker
            .await
            .expect("supervisor task should complete for panic case");

        tokio::time::sleep(Duration::from_millis(20)).await;

        assert_eq!(
            supervisor.lifecycle_of("panicking_worker"),
            Some(WorkerLifecycle::Panicked)
        );
        assert_eq!(process_health.current(), ProcessHealth::Degraded);
        assert!(!running_worker.is_finished());

        let _ = stop_tx.send(());
        running_worker
            .await
            .expect("supervisor task should complete after stop signal");
    }

    #[tokio::test]
    async fn worker_supervisor_marks_critical_failure_as_runtime_failed() {
        let supervisor = WorkerSupervisor::new();
        let process_health = Arc::new(ProcessHealthState::new(ProcessHealth::Healthy));

        let critical_worker = supervisor.spawn(
            "critical_worker",
            WorkerCriticality::Critical,
            Arc::clone(&process_health),
            || async {},
        );

        critical_worker
            .await
            .expect("supervisor task should complete for exited worker");

        assert_eq!(
            supervisor.lifecycle_of("critical_worker"),
            Some(WorkerLifecycle::Exited)
        );
        assert_eq!(process_health.current(), ProcessHealth::RuntimeFailed);
    }
}
