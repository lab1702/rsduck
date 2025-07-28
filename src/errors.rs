use thiserror::Error;
use axum::http::StatusCode;
use axum::response::{Json, Response, IntoResponse};
use serde::Serialize;

/// Database-related errors
#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Database connection pool error: {0}")]
    Pool(#[from] r2d2::Error),
    
    #[error("DuckDB error: {0}")]
    DuckDb(#[from] duckdb::Error),
    
    #[error("Task execution error: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),
    
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// API-level errors with structured responses
#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Bad Request: {message}")]
    BadRequest { message: String },
    
    #[error("Forbidden: {message}")]
    Forbidden { message: String },
    
    #[error("Internal Server Error: {message}")]
    InternalServerError { message: String },
    
    #[error("Database Error: {0}")]
    Database(#[from] DatabaseError),
}

/// Structured error response sent to clients
#[derive(Serialize)]
pub struct ErrorResponse {
    pub success: bool,
    pub error: ErrorDetail,
    pub query_id: Option<String>,
    pub timestamp: u64,
}

/// Detailed error information
#[derive(Serialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
}

impl ApiError {
    /// Create a bad request error
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest { message: message.into() }
    }

    /// Create a forbidden error  
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden { message: message.into() }
    }

    /// Create an internal server error
    pub fn internal_server_error(message: impl Into<String>) -> Self {
        Self::InternalServerError { message: message.into() }
    }
    
    /// Get the appropriate HTTP status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            ApiError::BadRequest { .. } => StatusCode::BAD_REQUEST,
            ApiError::Forbidden { .. } => StatusCode::FORBIDDEN,
            ApiError::InternalServerError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::Database(db_err) => {
                match db_err {
                    DatabaseError::Pool(_) => StatusCode::SERVICE_UNAVAILABLE,
                    DatabaseError::DuckDb(_) => StatusCode::BAD_REQUEST,
                    DatabaseError::TaskJoin(_) => StatusCode::INTERNAL_SERVER_ERROR,
                    DatabaseError::Json(_) => StatusCode::INTERNAL_SERVER_ERROR,
                }
            }
        }
    }
    
    /// Get error code for structured responses
    pub fn error_code(&self) -> &'static str {
        match self {
            ApiError::BadRequest { .. } => "BAD_REQUEST",
            ApiError::Forbidden { .. } => "FORBIDDEN",
            ApiError::InternalServerError { .. } => "INTERNAL_SERVER_ERROR",
            ApiError::Database(db_err) => {
                match db_err {
                    DatabaseError::Pool(_) => "DATABASE_POOL_ERROR",
                    DatabaseError::DuckDb(_) => "DATABASE_QUERY_ERROR",
                    DatabaseError::TaskJoin(_) => "TASK_EXECUTION_ERROR",
                    DatabaseError::Json(_) => "JSON_SERIALIZATION_ERROR",
                }
            }
        }
    }
    
    /// Convert error to HTTP response with optional query ID
    pub fn to_response(&self, query_id: Option<String>) -> Response {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
            
        let error_response = ErrorResponse {
            success: false,
            error: ErrorDetail {
                code: self.error_code().to_string(),
                message: self.to_string(),
                details: match self {
                    ApiError::Database(DatabaseError::DuckDb(e)) => {
                        // Provide sanitized database error details
                        Some(sanitize_database_error(e))
                    },
                    _ => None,
                },
            },
            query_id,
            timestamp,
        };
        
        (self.status_code(), Json(error_response)).into_response()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        self.to_response(None)
    }
}

impl From<DatabaseError> for String {
    fn from(err: DatabaseError) -> String {
        err.to_string()
    }
}

fn sanitize_database_error(error: &duckdb::Error) -> String {
    let error_str = error.to_string();
    
    // Remove potentially sensitive information from error messages
    if error_str.contains("does not exist") {
        return "Referenced table or column does not exist".to_string();
    }
    
    if error_str.contains("syntax error") || error_str.contains("parse") {
        return "SQL syntax error".to_string();
    }
    
    if error_str.contains("permission") || error_str.contains("access") {
        return "Access denied".to_string();
    }
    
    // For other errors, provide a generic message to avoid information leakage
    "Database query failed".to_string()
}