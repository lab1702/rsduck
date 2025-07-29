use crate::{AppState, DatabaseError};
use regex::Regex;
use serde_json;
use tracing::{debug, info, instrument, warn};

const DEFAULT_ROW_LIMIT: usize = 10000;
const MAX_ROW_LIMIT: usize = 100000;

/// Validate that a SQL operation is allowed in read-only mode
/// Returns an error message if the operation is not allowed, None otherwise
#[instrument(skip(state))]
pub fn validate_readonly_operation(state: &AppState, sql: &str) -> Option<String> {
    if !state.is_readonly {
        return None;
    }

    if is_write_operation(sql) {
        warn!("Write operation blocked on read-only database");
        Some("Database is opened in read-only mode. Write operations are not allowed.".to_string())
    } else {
        None
    }
}

fn is_write_operation(sql: &str) -> bool {
    // Remove SQL comments and normalize whitespace
    let cleaned_sql = remove_sql_comments(sql);

    // Split by semicolons to handle multiple statements
    let statements: Vec<&str> = cleaned_sql
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    // Check each statement for write operations
    for statement in statements {
        if is_single_statement_write_operation(statement) {
            return true;
        }
    }

    false
}

fn remove_sql_comments(sql: &str) -> String {
    // Remove /* */ block comments
    let block_comment_regex = Regex::new(r"/\*.*?\*/").unwrap();
    let mut cleaned = block_comment_regex.replace_all(sql, " ").to_string();

    // Remove -- line comments
    let line_comment_regex = Regex::new(r"--.*?(\n|$)").unwrap();
    cleaned = line_comment_regex.replace_all(&cleaned, "\n").to_string();

    // Normalize whitespace
    let whitespace_regex = Regex::new(r"\s+").unwrap();
    whitespace_regex
        .replace_all(&cleaned, " ")
        .trim()
        .to_string()
}

fn is_single_statement_write_operation(statement: &str) -> bool {
    let statement_upper = statement.to_uppercase();
    let statement_trimmed = statement_upper.trim();

    // Common write operations
    let write_patterns = [
        "INSERT",
        "UPDATE",
        "DELETE",
        "CREATE",
        "DROP",
        "ALTER",
        "TRUNCATE",
        "REPLACE",
        "MERGE",
        "UPSERT",
        // Transaction control that could enable writes
        "BEGIN",
        "START TRANSACTION",
        "COMMIT",
        "ROLLBACK",
        // DDL operations
        "GRANT",
        "REVOKE",
        // DuckDB specific write operations
        "COPY",
        "EXPORT",
        "IMPORT",
        "ATTACH",
        "DETACH",
    ];

    for pattern in &write_patterns {
        if statement_trimmed.starts_with(pattern) {
            // Additional validation for COPY - allow COPY TO but not COPY FROM
            if pattern == &"COPY" {
                // COPY FROM is a write operation, COPY TO is generally read-only
                if statement_trimmed.contains("FROM") && !statement_trimmed.contains("TO") {
                    return true;
                }
                if let (Some(from_pos), Some(to_pos)) =
                    (statement_trimmed.find("FROM"), statement_trimmed.find("TO"))
                {
                    if from_pos < to_pos {
                        return true;
                    }
                }
                continue;
            }
            return true;
        }
    }

    // Check for WITH clauses that might contain write operations
    if statement_trimmed.starts_with("WITH") {
        // Look for INSERT/UPDATE/DELETE in the final SELECT
        let with_regex =
            Regex::new(r"WITH\s+.*?\s+(INSERT|UPDATE|DELETE|CREATE|DROP|ALTER)").unwrap();
        if with_regex.is_match(&statement_upper) {
            return true;
        }
    }

    false
}

/// Execute a SQL query without a row limit
pub fn execute_sql(state: &AppState, sql: &str) -> Result<serde_json::Value, DatabaseError> {
    execute_sql_with_limit(state, sql, None)
}

/// Execute a SQL query with an optional row limit
/// If limit is provided, it will be clamped to MAX_ROW_LIMIT
/// If no limit is provided, DEFAULT_ROW_LIMIT is used

#[instrument(skip(state))]
pub fn execute_sql_with_limit(
    state: &AppState,
    sql: &str,
    row_limit: Option<usize>,
) -> Result<serde_json::Value, DatabaseError> {
    let limit = row_limit.unwrap_or(DEFAULT_ROW_LIMIT).min(MAX_ROW_LIMIT);

    debug!("Acquiring database connection from pool");
    let conn = state.pool.get()?;
    debug!("Preparing SQL statement");
    let mut stmt = conn.prepare(sql)?;

    debug!("Executing query");
    let rows = stmt.query_map([], |row| {
        let column_count = row.as_ref().column_count();
        let mut row_data = Vec::new();
        for i in 0..column_count {
            let value = convert_value_to_json(row.get_ref(i))?;
            row_data.push(value);
        }
        Ok((column_count, row_data))
    })?;

    let mut result_rows = Vec::new();
    let mut detected_column_count = 0;
    let mut row_count = 0;
    let mut truncated = false;

    debug!("Processing query results");
    for row_result in rows {
        if row_count >= limit {
            truncated = true;
            warn!("Query results truncated at {} rows", limit);
            break;
        }

        let (row_column_count, row_data) = row_result?;
        if detected_column_count == 0 {
            detected_column_count = row_column_count;
        }
        result_rows.push(row_data);
        row_count += 1;
    }

    let column_count = if detected_column_count > 0 {
        detected_column_count
    } else {
        stmt.column_count()
    };

    let column_names = get_column_names(&stmt, column_count)?;

    let mut response = serde_json::json!({
        "columns": column_names,
        "rows": result_rows,
        "row_count": result_rows.len(),
        "limit_applied": limit
    });

    if truncated {
        response["truncated"] = serde_json::Value::Bool(true);
        response["message"] = serde_json::Value::String(format!(
            "Results truncated to {} rows. Use limit parameter or streaming for larger datasets.",
            limit
        ));
    }

    info!(
        row_count = result_rows.len(),
        column_count = column_count,
        truncated = truncated,
        "Query execution completed"
    );

    Ok(response)
}

fn convert_value_to_json(
    value_ref_result: Result<duckdb::types::ValueRef, duckdb::Error>,
) -> Result<serde_json::Value, duckdb::Error> {
    match value_ref_result {
        Ok(value_ref) => {
            use duckdb::types::ValueRef;
            let json_value = match value_ref {
                ValueRef::Null => serde_json::Value::Null,
                ValueRef::Boolean(b) => serde_json::Value::Bool(b),
                ValueRef::TinyInt(i) => serde_json::Value::Number((i as i64).into()),
                ValueRef::SmallInt(i) => serde_json::Value::Number((i as i64).into()),
                ValueRef::Int(i) => serde_json::Value::Number((i as i64).into()),
                ValueRef::BigInt(i) => serde_json::Value::Number(i.into()),
                ValueRef::HugeInt(i) => serde_json::Value::Number((i as i64).into()),
                ValueRef::UTinyInt(i) => serde_json::Value::Number((i as i64).into()),
                ValueRef::USmallInt(i) => serde_json::Value::Number((i as i64).into()),
                ValueRef::UInt(i) => serde_json::Value::Number((i as i64).into()),
                ValueRef::UBigInt(i) => serde_json::Value::Number((i as i64).into()),
                ValueRef::Float(f) => match serde_json::Number::from_f64(f as f64) {
                    Some(num) => serde_json::Value::Number(num),
                    None => serde_json::Value::Null,
                },
                ValueRef::Double(f) => match serde_json::Number::from_f64(f) {
                    Some(num) => serde_json::Value::Number(num),
                    None => serde_json::Value::Null,
                },
                ValueRef::Text(s) => {
                    serde_json::Value::String(String::from_utf8_lossy(s).to_string())
                }
                ValueRef::Blob(b) => {
                    // For security, don't expose raw blob data
                    // Instead provide metadata about the blob
                    serde_json::Value::String(format!("<BLOB {} bytes>", b.len()))
                }
                _ => {
                    // For security, don't expose raw debug info for unknown types
                    // Instead provide a safe generic message
                    serde_json::Value::String("<UNSUPPORTED_TYPE>".to_string())
                }
            };
            Ok(json_value)
        }
        Err(e) => Err(e),
    }
}

fn get_column_names(
    stmt: &duckdb::Statement,
    column_count: usize,
) -> Result<Vec<String>, DatabaseError> {
    let mut column_names = Vec::new();
    for i in 0..column_count {
        let column_name = match stmt.column_name(i) {
            Ok(name) => name.to_string(),
            Err(_) => format!("column_{}", i),
        };
        column_names.push(column_name);
    }
    Ok(column_names)
}

#[instrument(skip(state))]
pub fn execute_sql_command(
    state: &AppState,
    sql: &str,
) -> Result<serde_json::Value, DatabaseError> {
    debug!("Acquiring database connection from pool for command execution");
    let conn = state.pool.get()?;

    debug!("Executing SQL command");
    let updated = conn.execute(sql, [])?;

    info!(rows_affected = updated, "Command execution completed");

    Ok(serde_json::json!({
        "rows": [],
        "row_count": 0,
        "rows_affected": updated
    }))
}
