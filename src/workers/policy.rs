/// Worker criticality used by startup and runtime failure policy decisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerCriticality {
    /// Required worker. If this cannot start, bootstrap should fail fast.
    Critical,
    /// Optional worker. If this cannot start, disable it and keep API serving.
    NonCritical,
}

/// Startup action chosen when a worker fails during bootstrap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapFailureAction {
    /// Abort process startup.
    FailFast,
    /// Mark worker disabled and continue booting.
    DisableAndContinue,
}

/// Runtime status of an individual worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerStatus {
    /// Worker is operating normally.
    Healthy,
    /// Worker is currently impaired but still retrying/operational.
    Degraded,
    /// Worker is disabled and should not continue processing.
    Disabled,
}

/// Why a worker became degraded or disabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DegradationReason {
    /// Downstream Solana/Postgres/external RPC temporarily unavailable.
    RpcUnavailable,
    /// Worker is retrying with backoff after transient failures.
    RetryBackoff,
    /// Safety threshold reached (e.g. low vault SOL circuit breaker).
    CircuitBreakerTripped,
    /// Worker panic was caught and worker should be disabled.
    PanicRecovered,
}

impl WorkerCriticality {
    /// Policy for worker initialization failures at bootstrap time.
    #[must_use]
    pub const fn on_bootstrap_failure(self) -> BootstrapFailureAction {
        match self {
            Self::Critical => BootstrapFailureAction::FailFast,
            Self::NonCritical => BootstrapFailureAction::DisableAndContinue,
        }
    }

    /// Policy for runtime worker failures.
    ///
    /// Runtime worker failure should disable that worker and keep API running.
    #[must_use]
    pub const fn on_runtime_failure(self) -> WorkerStatus {
        let _ = self;
        WorkerStatus::Disabled
    }
}

impl DegradationReason {
    /// Suggested worker status for this degradation reason.
    #[must_use]
    pub const fn suggested_status(self) -> WorkerStatus {
        match self {
            Self::RpcUnavailable | Self::RetryBackoff => WorkerStatus::Degraded,
            Self::CircuitBreakerTripped | Self::PanicRecovered => WorkerStatus::Disabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn critical_workers_fail_fast_on_bootstrap_failure() {
        assert_eq!(
            WorkerCriticality::Critical.on_bootstrap_failure(),
            BootstrapFailureAction::FailFast
        );
    }

    #[test]
    fn non_critical_workers_disable_on_bootstrap_failure() {
        assert_eq!(
            WorkerCriticality::NonCritical.on_bootstrap_failure(),
            BootstrapFailureAction::DisableAndContinue
        );
    }

    #[test]
    fn runtime_failure_disables_worker_for_both_criticalities() {
        assert_eq!(
            WorkerCriticality::Critical.on_runtime_failure(),
            WorkerStatus::Disabled
        );
        assert_eq!(
            WorkerCriticality::NonCritical.on_runtime_failure(),
            WorkerStatus::Disabled
        );
    }

    #[test]
    fn degraded_reasons_map_to_degraded_status() {
        assert_eq!(
            DegradationReason::RpcUnavailable.suggested_status(),
            WorkerStatus::Degraded
        );
        assert_eq!(
            DegradationReason::RetryBackoff.suggested_status(),
            WorkerStatus::Degraded
        );
    }

    #[test]
    fn disabling_reasons_map_to_disabled_status() {
        assert_eq!(
            DegradationReason::CircuitBreakerTripped.suggested_status(),
            WorkerStatus::Disabled
        );
        assert_eq!(
            DegradationReason::PanicRecovered.suggested_status(),
            WorkerStatus::Disabled
        );
    }

    #[test]
    fn policy_types_are_send_and_sync() {
        assert_send_sync::<WorkerCriticality>();
        assert_send_sync::<WorkerStatus>();
        assert_send_sync::<DegradationReason>();
        assert_send_sync::<BootstrapFailureAction>();
    }
}
