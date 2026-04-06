//! # ef-control
//!
//! Device control plane — the core of EdgeFabric.
//! Handles device discovery, health monitoring, command execution, and state management.

pub mod command;
pub mod discovery;
pub mod pool;
pub mod snapshot;
pub mod state;
