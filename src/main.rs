use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use clap::Parser;
use duckdb::{Connection, Result as DuckResult, Config};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::SystemTime,
};
use tower_http::cors::CorsLayer;
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "rsduck")]
#[command(about = "A DuckDB REST server")]
#[command(version = "1.0")]
struct Args {
    /// DuckDB database file path (uses in-memory database if not specified)
    #[arg(short, long)]
    database: Option<PathBuf>,

    /// Open database in read-write mode (default is read-only for file databases)
    #[arg(long)]
    readwrite: bool,

    /// Server port
    #[arg(short, long, default_value = "3001")]
    port: u16,

    /// Server host
    #[arg(long, default_value = "0.0.0.0")]
    host: String,
}

#[derive(Clone)]
pub struct AppState {
    db: Arc<Mutex<Connection>>,
    db_path: Option<PathBuf>,
    is_readonly: bool,
}

impl AppState {
    pub fn new(args: &Args) -> anyhow::Result<Self> {
        let conn = match &args.database {
            Some(path) => {
                if args.readwrite {
                    println!("Opening database file: {:?} (read-write)", path);
                    Connection::open(path)?
                } else {
                    println!("Opening database file: {:?} (read-only)", path);
                    let config = Config::default().access_mode(duckdb::AccessMode::ReadOnly)?;
                    Connection::open_with_flags(path, config)?
                }
            }
            None => {
                println!("Using in-memory database (read-write)");
                Connection::open_in_memory()?
            }
        };

        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
            db_path: args.database.clone(),
            is_readonly: args.database.is_some() && !args.readwrite,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct QueryRequest {
    sql: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct QueryParams {
    sql: Option<String>,
}

#[derive(Debug, Serialize)]
struct QueryResponse {
    success: bool,
    data: Option<serde_json::Value>,
    error: Option<String>,
    query_id: String,
    execution_time_ms: u64,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    timestamp: u64,
    database_path: Option<String>,
    readonly_mode: bool,
}

async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        database_path: state.db_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        readonly_mode: state.is_readonly,
    })
}

async fn execute_query_post(
    State(state): State<AppState>,
    Json(request): Json<QueryRequest>,
) -> (StatusCode, Json<QueryResponse>) {
    execute_query_internal(state, request.sql).await
}

async fn execute_query_get(
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> (StatusCode, Json<QueryResponse>) {
    match params.sql {
        Some(sql) => execute_query_internal(state, sql).await,
        None => (
            StatusCode::BAD_REQUEST,
            Json(QueryResponse {
                success: false,
                data: None,
                error: Some("Missing 'sql' parameter".to_string()),
                query_id: Uuid::new_v4().to_string(),
                execution_time_ms: 0,
            }),
        ),
    }
}

async fn execute_query_internal(
    state: AppState,
    sql: String,
) -> (StatusCode, Json<QueryResponse>) {
    let query_id = Uuid::new_v4().to_string();
    let start_time = SystemTime::now();

    // Check if it's a write operation on a readonly database
    if state.is_readonly {
        let sql_upper = sql.trim().to_uppercase();
        if sql_upper.starts_with("INSERT") || 
           sql_upper.starts_with("UPDATE") || 
           sql_upper.starts_with("DELETE") || 
           sql_upper.starts_with("CREATE") || 
           sql_upper.starts_with("DROP") || 
           sql_upper.starts_with("ALTER") {
            return (
                StatusCode::FORBIDDEN,
                Json(QueryResponse {
                    success: false,
                    data: None,
                    error: Some("Database is opened in read-only mode. Write operations are not allowed.".to_string()),
                    query_id,
                    execution_time_ms: 0,
                }),
            );
        }
    }

    let result = tokio::task::spawn_blocking(move || {
        execute_sql(&state, &sql)
    }).await;

    let execution_time_ms = start_time.elapsed().unwrap_or_default().as_millis() as u64;

    match result {
        Ok(sql_result) => {
            match sql_result {
                Ok(data) => (
                    StatusCode::OK,
                    Json(QueryResponse {
                        success: true,
                        data: Some(data),
                        error: None,
                        query_id,
                        execution_time_ms,
                    }),
                ),
                Err(e) => (
                    StatusCode::BAD_REQUEST,
                    Json(QueryResponse {
                        success: false,
                        data: None,
                        error: Some(e.to_string()),
                        query_id,
                        execution_time_ms,
                    }),
                ),
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryResponse {
                success: false,
                data: None,
                error: Some(format!("Task execution error: {}", e)),
                query_id,
                execution_time_ms,
            }),
        ),
    }
}

fn execute_sql(state: &AppState, sql: &str) -> DuckResult<serde_json::Value> {
    let conn = state.db.lock().unwrap();
    
    // First try to execute as a non-query (CREATE, INSERT, UPDATE, DELETE, etc.)
    if let Ok(updated) = conn.execute(sql, []) {
        return Ok(serde_json::json!({
            "rows": [],
            "row_count": 0,
            "rows_affected": updated
        }));
    }

    // If that fails, try as a query (SELECT)
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| {
        // Simple approach: just get the first column value
        let value = match row.get::<usize, String>(0) {
            Ok(s) => serde_json::Value::String(s),
            Err(_) => {
                // Try as integer
                match row.get::<usize, i64>(0) {
                    Ok(i) => serde_json::Value::Number(i.into()),
                    Err(_) => serde_json::Value::String("result".to_string()),
                }
            }
        };
        Ok(vec![value])
    })?;

    let mut result_rows = Vec::new();
    for row_result in rows {
        result_rows.push(row_result?);
    }

    Ok(serde_json::json!({
        "rows": result_rows,
        "row_count": result_rows.len()
    }))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let state = AppState::new(&args)?;

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/query", post(execute_query_post))
        .route("/query", get(execute_query_get))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let bind_addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    
    println!("DuckDB REST server running on http://{}", bind_addr);
    println!("Endpoints:");
    println!("  GET  /health - Health check");
    println!("  POST /query  - Execute SQL query (JSON body)");
    println!("  GET  /query?sql=<query> - Execute SQL query (URL parameter)");
    println!();
    println!("Usage examples:");
    println!("  cargo run                                    # In-memory database");
    println!("  cargo run -- --database mydb.duckdb         # Read-only file");
    println!("  cargo run -- --database mydb.duckdb --readwrite  # Read-write file");
    println!("  cargo run -- --port 8080                    # Custom port");

    axum::serve(listener, app).await?;

    Ok(())
}
