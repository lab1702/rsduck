//! RSDuck - A secure DuckDB REST API server
//!
//! This crate provides a REST API server for DuckDB with security features,
//! connection pooling, and comprehensive logging.

/// Database operations and connection management
pub mod database;
/// Error types and handling
pub mod errors;
/// HTTP request handlers
pub mod handlers;
/// Data models and configuration
pub mod models;

pub use database::*;
pub use errors::{ApiError, DatabaseError};
pub use handlers::*;
pub use models::*;
