use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use tokio::{task::JoinHandle, time::timeout};

pub type WorkerFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

#[derive(Clone)]
pub struct WorkerSpec {
    pub name: &'static str,
    starter: Arc<dyn Fn() -> WorkerFuture + Send + Sync>,
}

impl WorkerSpec {
    pub fn new<F, Fut>(name: &'static str, starter: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Self {
            name,
            starter: Arc::new(move || Box::pin(starter())),
        }
    }

    pub fn spawn(&self) -> SpawnedWorker {
        SpawnedWorker {
            name: self.name,
            handle: tokio::spawn((self.starter)()),
        }
    }
}

pub struct SpawnedWorker {
    pub name: &'static str,
    handle: JoinHandle<()>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerProbeState {
    Running,
    Completed,
    Panicked,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerProbeResult {
    pub worker: &'static str,
    pub state: WorkerProbeState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerCriticality {
    Critical,
    NonCritical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessHealth {
    Healthy,
    Degraded,
    RuntimeFailed,
    StartupFailed,
}

#[derive(Clone)]
pub struct SupervisedWorkerSpec {
    pub worker: WorkerSpec,
    pub criticality: WorkerCriticality,
}

impl SupervisedWorkerSpec {
    pub fn new(worker: WorkerSpec, criticality: WorkerCriticality) -> Self {
        Self {
            worker,
            criticality,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisionSummary {
    pub process_health: ProcessHealth,
    pub results: Vec<WorkerProbeResult>,
}

#[derive(Debug, Clone, Copy)]
pub struct BootstrapCheck {
    pub name: &'static str,
    pub required: bool,
    pub passed: bool,
}

pub fn evaluate_bootstrap(checks: &[BootstrapCheck]) -> Result<(), &'static str> {
    for check in checks {
        if check.required && !check.passed {
            return Err(check.name);
        }
    }

    Ok(())
}

pub async fn spawn_and_probe(
    specs: &[WorkerSpec],
    probe_window: Duration,
) -> Vec<WorkerProbeResult> {
    let mut spawned: Vec<SpawnedWorker> = specs.iter().map(WorkerSpec::spawn).collect();
    let mut results = Vec::with_capacity(spawned.len());

    for worker in &mut spawned {
        let state = match timeout(probe_window, &mut worker.handle).await {
            Err(_) => WorkerProbeState::Running,
            Ok(Ok(())) => WorkerProbeState::Completed,
            Ok(Err(join_error)) if join_error.is_panic() => WorkerProbeState::Panicked,
            Ok(Err(_)) => WorkerProbeState::Cancelled,
        };

        results.push(WorkerProbeResult {
            worker: worker.name,
            state,
        });

        if !worker.handle.is_finished() {
            worker.handle.abort();
            let _ = (&mut worker.handle).await;
        }
    }

    results
}

pub async fn spawn_and_supervise(
    specs: &[SupervisedWorkerSpec],
    probe_window: Duration,
) -> SupervisionSummary {
    let worker_specs: Vec<WorkerSpec> = specs.iter().map(|spec| spec.worker.clone()).collect();
    let results = spawn_and_probe(&worker_specs, probe_window).await;

    let mut process_health = ProcessHealth::Healthy;

    for (spec, result) in specs.iter().zip(results.iter()) {
        let worker_failed = matches!(
            result.state,
            WorkerProbeState::Completed | WorkerProbeState::Panicked | WorkerProbeState::Cancelled
        );

        if !worker_failed {
            continue;
        }

        match spec.criticality {
            WorkerCriticality::NonCritical
                if process_health != ProcessHealth::StartupFailed
                    && process_health != ProcessHealth::RuntimeFailed =>
            {
                process_health = ProcessHealth::Degraded;
            }
            WorkerCriticality::Critical => {
                process_health = ProcessHealth::RuntimeFailed;
            }
            WorkerCriticality::NonCritical => {}
        }
    }

    SupervisionSummary {
        process_health,
        results,
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ScriptedWorkerOutcome {
    KeepRunning,
    Exit,
    Panic,
}

pub fn scripted_worker(name: &'static str, outcome: ScriptedWorkerOutcome) -> WorkerSpec {
    WorkerSpec::new(name, move || async move {
        match outcome {
            ScriptedWorkerOutcome::KeepRunning => {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
            ScriptedWorkerOutcome::Exit => {}
            ScriptedWorkerOutcome::Panic => panic!("scripted hard failure in worker {name}"),
        }
    })
}

pub fn supervised_scripted_worker(
    name: &'static str,
    criticality: WorkerCriticality,
    outcome: ScriptedWorkerOutcome,
) -> SupervisedWorkerSpec {
    SupervisedWorkerSpec::new(scripted_worker(name, outcome), criticality)
}
