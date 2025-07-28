//! RSDuck - A secure DuckDB REST API server
//! 
//! This crate provides a REST API server for DuckDB with security features,
//! connection pooling, and comprehensive logging.

/// Data models and configuration
pub mod models;
/// Database operations and connection management
pub mod database;
/// HTTP request handlers
pub mod handlers;
/// Error types and handling
pub mod errors;

pub use models::*;
pub use database::*;
pub use handlers::*;
pub use errors::{DatabaseError, ApiError};