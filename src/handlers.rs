use axum::{
    extract::{Query, State},
    response::{Json, Response},
};
use std::time::SystemTime;
use tracing::{error, info, instrument, warn};
use utoipa;
use uuid::Uuid;

use crate::database::{execute_sql_command, execute_sql_with_limit, validate_readonly_operation};
use crate::{ApiError, AppState, HealthResponse, QueryParams, QueryRequest, QueryResponse};

/// Health check endpoint handler
/// Returns server status, timestamp, database info, and read-only mode status
#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Server health status", body = HealthResponse)
    ),
    tag = "health"
)]
#[instrument(skip(state))]
pub async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    info!("Health check requested");

    Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp,
        database_path: state
            .db_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        readonly_mode: state.is_readonly,
    })
}

/// POST endpoint handler for SQL query execution
/// Accepts SQL queries in request body with optional row limit
#[utoipa::path(
    post,
    path = "/query",
    request_body = QueryRequest,
    responses(
        (status = 200, description = "Query executed successfully", body = QueryResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Operation forbidden in read-only mode"),
        (status = 500, description = "Internal server error")
    ),
    tag = "query"
)]
#[instrument(skip(state, request), fields(sql_length = request.sql.len(), limit = request.limit))]
pub async fn execute_query_post(
    State(state): State<AppState>,
    Json(request): Json<QueryRequest>,
) -> Result<Json<QueryResponse>, Response> {
    info!("Query execution requested via POST");
    execute_query_internal(state, request.sql, request.limit).await
}

/// GET endpoint handler for SQL query execution
/// Accepts SQL queries as URL parameters with optional row limit
#[utoipa::path(
    get,
    path = "/query",
    params(
        ("sql" = Option<String>, Query, description = "SQL query to execute"),
        ("limit" = Option<usize>, Query, description = "Maximum number of rows to return")
    ),
    responses(
        (status = 200, description = "Query executed successfully", body = QueryResponse),
        (status = 400, description = "Bad request - missing SQL parameter"),
        (status = 403, description = "Operation forbidden in read-only mode"),
        (status = 500, description = "Internal server error")
    ),
    tag = "query"
)]
#[instrument(skip(state, params), fields(sql_length = params.sql.as_ref().map(|s| s.len()), limit = params.limit))]
pub async fn execute_query_get(
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<QueryResponse>, Response> {
    info!("Query execution requested via GET");
    match params.sql {
        Some(sql) => execute_query_internal(state, sql, params.limit).await,
        None => {
            let query_id = Uuid::new_v4().to_string();
            warn!("Query request missing SQL parameter");
            let error = ApiError::bad_request("Missing 'sql' parameter");
            Err(error.to_response(Some(query_id)))
        }
    }
}

#[instrument(skip(state, sql), fields(query_id, sql_preview = %sql.chars().take(50).collect::<String>(), limit))]
async fn execute_query_internal(
    state: AppState,
    sql: String,
    limit: Option<usize>,
) -> Result<Json<QueryResponse>, Response> {
    let query_id = Uuid::new_v4().to_string();
    tracing::Span::current().record("query_id", &query_id);
    tracing::Span::current().record("limit", &limit);

    let start_time = SystemTime::now();
    info!("Starting query execution");

    // Validate read-only operations
    if let Some(error_msg) = validate_readonly_operation(&state, &sql) {
        warn!("Read-only violation detected");
        let error = ApiError::forbidden(error_msg);
        return Err(error.to_response(Some(query_id)));
    }

    // Execute query in blocking task
    let result =
        tokio::task::spawn_blocking(move || execute_sql_with_limit(&state, &sql, limit)).await;

    let execution_time_ms = start_time.elapsed().unwrap_or_default().as_millis() as u64;

    match result {
        Ok(sql_result) => match sql_result {
            Ok(data) => {
                let row_count = data.get("row_count").and_then(|v| v.as_u64()).unwrap_or(0);
                let truncated = data
                    .get("truncated")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                info!(
                    execution_time_ms = execution_time_ms,
                    row_count = row_count,
                    truncated = truncated,
                    "Query executed successfully"
                );

                Ok(Json(QueryResponse {
                    success: true,
                    data: Some(data),
                    error: None,
                    query_id,
                    execution_time_ms,
                }))
            }
            Err(e) => {
                error!(
                    execution_time_ms = execution_time_ms,
                    error = %e,
                    "Query execution failed"
                );
                let error = ApiError::Database(e);
                Err(error.to_response(Some(query_id)))
            }
        },
        Err(e) => {
            error!(
                execution_time_ms = execution_time_ms,
                error = %e,
                "Task execution failed"
            );
            let error = ApiError::internal_server_error(format!("Task execution error: {}", e));
            Err(error.to_response(Some(query_id)))
        }
    }
}

#[utoipa::path(
    post,
    path = "/execute",
    request_body = QueryRequest,
    responses(
        (status = 200, description = "Command executed successfully", body = QueryResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Operation forbidden in read-only mode"),
        (status = 500, description = "Internal server error")
    ),
    tag = "execute"
)]
#[instrument(skip(state, request), fields(sql_length = request.sql.len()))]
pub async fn execute_command_post(
    State(state): State<AppState>,
    Json(request): Json<QueryRequest>,
) -> Result<Json<QueryResponse>, Response> {
    info!("Command execution requested via POST");
    execute_command_internal(state, request.sql).await
}

#[utoipa::path(
    get,
    path = "/execute",
    params(
        ("sql" = Option<String>, Query, description = "SQL command to execute")
    ),
    responses(
        (status = 200, description = "Command executed successfully", body = QueryResponse),
        (status = 400, description = "Bad request - missing SQL parameter"),
        (status = 403, description = "Operation forbidden in read-only mode"),
        (status = 500, description = "Internal server error")
    ),
    tag = "execute"
)]
#[instrument(skip(state, params), fields(sql_length = params.sql.as_ref().map(|s| s.len())))]
pub async fn execute_command_get(
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<QueryResponse>, Response> {
    info!("Command execution requested via GET");
    match params.sql {
        Some(sql) => execute_command_internal(state, sql).await,
        None => {
            let query_id = Uuid::new_v4().to_string();
            warn!("Command request missing SQL parameter");
            let error = ApiError::bad_request("Missing 'sql' parameter");
            Err(error.to_response(Some(query_id)))
        }
    }
}

#[instrument(skip(state, sql), fields(query_id, sql_preview = %sql.chars().take(50).collect::<String>()))]
async fn execute_command_internal(
    state: AppState,
    sql: String,
) -> Result<Json<QueryResponse>, Response> {
    let query_id = Uuid::new_v4().to_string();
    tracing::Span::current().record("query_id", &query_id);

    let start_time = SystemTime::now();
    info!("Starting command execution");

    // Validate read-only operations
    if let Some(error_msg) = validate_readonly_operation(&state, &sql) {
        warn!("Read-only violation detected");
        let error = ApiError::forbidden(error_msg);
        return Err(error.to_response(Some(query_id)));
    }

    // Execute command in blocking task
    let result = tokio::task::spawn_blocking(move || execute_sql_command(&state, &sql)).await;

    let execution_time_ms = start_time.elapsed().unwrap_or_default().as_millis() as u64;

    match result {
        Ok(sql_result) => match sql_result {
            Ok(data) => {
                let rows_affected = data
                    .get("rows_affected")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                info!(
                    execution_time_ms = execution_time_ms,
                    rows_affected = rows_affected,
                    "Command executed successfully"
                );

                Ok(Json(QueryResponse {
                    success: true,
                    data: Some(data),
                    error: None,
                    query_id,
                    execution_time_ms,
                }))
            }
            Err(e) => {
                error!(
                    execution_time_ms = execution_time_ms,
                    error = %e,
                    "Command execution failed"
                );
                let error = ApiError::Database(e);
                Err(error.to_response(Some(query_id)))
            }
        },
        Err(e) => {
            error!(
                execution_time_ms = execution_time_ms,
                error = %e,
                "Task execution failed"
            );
            let error = ApiError::internal_server_error(format!("Task execution error: {}", e));
            Err(error.to_response(Some(query_id)))
        }
    }
}
