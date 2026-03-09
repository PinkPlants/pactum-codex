mod support {
    pub mod worker_harness;
}

use std::time::Duration;

use support::worker_harness::{
    evaluate_bootstrap, scripted_worker, spawn_and_probe, spawn_and_supervise,
    supervised_scripted_worker, BootstrapCheck, ProcessHealth, ScriptedWorkerOutcome,
    WorkerCriticality, WorkerProbeState,
};

#[tokio::test]
async fn worker_probe_flags_hard_failures() {
    let workers = vec![
        scripted_worker("event_listener", ScriptedWorkerOutcome::KeepRunning),
        scripted_worker("refund_worker", ScriptedWorkerOutcome::Panic),
    ];

    let results = spawn_and_probe(&workers, Duration::from_millis(25)).await;

    assert!(results
        .iter()
        .any(|r| r.worker == "event_listener" && r.state == WorkerProbeState::Running));
    assert!(results
        .iter()
        .any(|r| r.worker == "refund_worker" && r.state == WorkerProbeState::Panicked));
}

#[tokio::test]
async fn worker_probe_flags_unexpected_early_exit() {
    let workers = vec![scripted_worker("keeper", ScriptedWorkerOutcome::Exit)];

    let results = spawn_and_probe(&workers, Duration::from_millis(25)).await;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].worker, "keeper");
    assert_eq!(results[0].state, WorkerProbeState::Completed);
}

#[tokio::test]
async fn worker_probe_handles_mixed_lifecycle_states() {
    let workers = vec![
        scripted_worker("notification_worker", ScriptedWorkerOutcome::KeepRunning),
        scripted_worker("expiry_worker", ScriptedWorkerOutcome::Exit),
        scripted_worker("keeper", ScriptedWorkerOutcome::Panic),
    ];

    let results = spawn_and_probe(&workers, Duration::from_millis(25)).await;

    assert_eq!(results.len(), 3);
    assert!(results
        .iter()
        .any(|r| r.worker == "notification_worker" && r.state == WorkerProbeState::Running));
    assert!(results
        .iter()
        .any(|r| r.worker == "expiry_worker" && r.state == WorkerProbeState::Completed));
    assert!(results
        .iter()
        .any(|r| r.worker == "keeper" && r.state == WorkerProbeState::Panicked));
}

#[test]
fn startup_fails_fast_when_required_bootstrap_dependency_fails() {
    let checks = vec![
        BootstrapCheck {
            name: "config",
            required: true,
            passed: true,
        },
        BootstrapCheck {
            name: "database",
            required: true,
            passed: false,
        },
        BootstrapCheck {
            name: "notification_provider",
            required: false,
            passed: false,
        },
    ];

    let failed = evaluate_bootstrap(&checks).expect_err("required bootstrap should fail fast");
    assert_eq!(failed, "database");
}

#[tokio::test]
async fn startup_succeeds_then_non_critical_runtime_failure_degrades_process() {
    let checks = vec![
        BootstrapCheck {
            name: "config",
            required: true,
            passed: true,
        },
        BootstrapCheck {
            name: "database",
            required: true,
            passed: true,
        },
        BootstrapCheck {
            name: "vault_keypair",
            required: true,
            passed: true,
        },
    ];

    evaluate_bootstrap(&checks).expect("required bootstrap checks must pass");

    let workers = vec![
        supervised_scripted_worker(
            "event_listener",
            WorkerCriticality::NonCritical,
            ScriptedWorkerOutcome::KeepRunning,
        ),
        supervised_scripted_worker(
            "notification_worker",
            WorkerCriticality::NonCritical,
            ScriptedWorkerOutcome::Panic,
        ),
        supervised_scripted_worker(
            "keeper",
            WorkerCriticality::Critical,
            ScriptedWorkerOutcome::KeepRunning,
        ),
    ];

    let summary = spawn_and_supervise(&workers, Duration::from_millis(25)).await;

    assert_eq!(summary.process_health, ProcessHealth::Degraded);
    assert!(summary
        .results
        .iter()
        .any(|r| r.worker == "notification_worker" && r.state == WorkerProbeState::Panicked));
    assert!(summary
        .results
        .iter()
        .any(|r| r.worker == "keeper" && r.state == WorkerProbeState::Running));
}

#[tokio::test]
async fn simultaneous_non_critical_worker_failures_remain_non_fatal() {
    let workers = vec![
        supervised_scripted_worker(
            "event_listener",
            WorkerCriticality::NonCritical,
            ScriptedWorkerOutcome::Panic,
        ),
        supervised_scripted_worker(
            "refund_worker",
            WorkerCriticality::NonCritical,
            ScriptedWorkerOutcome::Exit,
        ),
        supervised_scripted_worker(
            "keeper",
            WorkerCriticality::Critical,
            ScriptedWorkerOutcome::KeepRunning,
        ),
    ];

    let summary = spawn_and_supervise(&workers, Duration::from_millis(25)).await;

    assert_eq!(summary.process_health, ProcessHealth::Degraded);
    assert!(summary
        .results
        .iter()
        .any(|r| r.worker == "event_listener" && r.state == WorkerProbeState::Panicked));
    assert!(summary
        .results
        .iter()
        .any(|r| r.worker == "refund_worker" && r.state == WorkerProbeState::Completed));
}

#[tokio::test]
async fn critical_worker_failure_escalates_health_to_runtime_failed() {
    let workers = vec![
        supervised_scripted_worker(
            "event_listener",
            WorkerCriticality::NonCritical,
            ScriptedWorkerOutcome::Panic,
        ),
        supervised_scripted_worker(
            "keeper",
            WorkerCriticality::Critical,
            ScriptedWorkerOutcome::Exit,
        ),
        supervised_scripted_worker(
            "notification_worker",
            WorkerCriticality::NonCritical,
            ScriptedWorkerOutcome::KeepRunning,
        ),
    ];

    let summary = spawn_and_supervise(&workers, Duration::from_millis(25)).await;

    assert_eq!(summary.process_health, ProcessHealth::RuntimeFailed);
    assert!(summary
        .results
        .iter()
        .any(|r| r.worker == "keeper" && r.state == WorkerProbeState::Completed));
}
