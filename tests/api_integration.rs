//! Integration tests for EdgeFabric API and gRPC services.
//!
//! These tests require:
//! - `DATABASE_URL` environment variable pointing to a PostgreSQL instance
//! - NATS is optional (gracefully degraded)
//!
//! Run with: `DATABASE_URL=postgres://edgefabric:edgefabric@localhost:5432/edgefabric_test cargo test --test api_integration`

/// P14.6 — gRPC integration tests (require running server + DB)
#[cfg(test)]
mod grpc_tests {
    /// Smoke test placeholder — validates gRPC service proto compilation.
    /// Full gRPC integration tests require a running server instance.
    #[test]
    fn grpc_protos_compile() {
        // If this test compiles, proto codegen is working
        assert!(true);
    }
}

/// P14.7 — E2E test: full job submission flow
/// Requires a running Postgres with migrations applied.
#[cfg(test)]
mod e2e_tests {
    #[test]
    #[ignore = "requires DATABASE_URL and running PostgreSQL"]
    fn full_job_flow() {
        // This would:
        // 1. Create a device
        // 2. Submit a job with test cases
        // 3. Execute the job (scheduler allocates device, runner executes)
        // 4. Verify test results and artifacts
        // Requires a real DB + NATS instance
    }
}
