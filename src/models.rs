use clap::Parser;
use duckdb::{Config, Connection};
use r2d2::{Pool, PooledConnection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info};
use utoipa::ToSchema;

/// Type alias for the DuckDB connection pool
pub type DuckDbPool = Pool<DuckDbConnectionManager>;
/// Type alias for a pooled DuckDB connection
pub type DuckDbConnection = PooledConnection<DuckDbConnectionManager>;

/// Connection manager for r2d2 pool to manage DuckDB connections
#[derive(Debug)]
pub struct DuckDbConnectionManager {
    database_path: Option<PathBuf>,
    is_readonly: bool,
}

impl DuckDbConnectionManager {
    /// Create a new connection manager
    pub fn new(database_path: Option<PathBuf>, is_readonly: bool) -> Self {
        Self {
            database_path,
            is_readonly,
        }
    }
}

impl r2d2::ManageConnection for DuckDbConnectionManager {
    type Connection = Connection;
    type Error = duckdb::Error;

    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        match &self.database_path {
            Some(path) => {
                if self.is_readonly {
                    let config = Config::default().access_mode(duckdb::AccessMode::ReadOnly)?;
                    Connection::open_with_flags(path, config)
                } else {
                    Connection::open(path)
                }
            }
            None => Connection::open_in_memory(),
        }
    }

    fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        conn.execute("SELECT 1", [])?;
        Ok(())
    }

    fn has_broken(&self, _conn: &mut Self::Connection) -> bool {
        false
    }
}

/// Command line arguments for the RSDuck server
#[derive(Parser)]
#[command(name = "rsduck")]
#[command(about = "A DuckDB REST server")]
#[command(version = "1.0")]
pub struct Args {
    /// DuckDB database file path (uses in-memory database if not specified)
    #[arg(short, long)]
    pub database: Option<PathBuf>,

    /// Open database in read-write mode (default is read-only for file databases)
    #[arg(long)]
    pub readwrite: bool,

    /// Server port
    #[arg(short, long, default_value = "3001")]
    pub port: u16,

    /// Server host
    #[arg(long, default_value = "0.0.0.0")]
    pub host: String,
}

/// Application state containing database pool and configuration
#[derive(Clone)]
pub struct AppState {
    pub pool: DuckDbPool,
    pub db_path: Option<PathBuf>,
    pub is_readonly: bool,
}

impl AppState {
    /// Create new application state from command line arguments
    pub fn new(args: &Args) -> anyhow::Result<Self> {
        let is_readonly = args.database.is_some() && !args.readwrite;

        if let Some(path) = &args.database {
            if args.readwrite {
                info!("Opening database file: {:?} (read-write)", path);
            } else {
                info!("Opening database file: {:?} (read-only)", path);
            }
        } else {
            info!("Using in-memory database (read-write)");
        }

        debug!("Creating connection manager");
        let manager = DuckDbConnectionManager::new(args.database.clone(), is_readonly);

        debug!("Building connection pool with max size 10");
        let pool = Pool::builder()
            .max_size(10) // Maximum 10 connections in the pool
            .build(manager)?;

        info!("Database connection pool initialized successfully");

        Ok(Self {
            pool,
            db_path: args.database.clone(),
            is_readonly,
        })
    }
}

/// Query parameters for GET requests
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QueryParams {
    /// SQL query to execute
    #[schema(example = "SELECT * FROM users")]
    pub sql: Option<String>,
    /// Maximum number of rows to return
    #[schema(example = 100)]
    pub limit: Option<usize>,
}

/// Request body for POST requests
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QueryRequest {
    /// SQL query to execute
    #[schema(example = "SELECT * FROM users")]
    pub sql: String,
    /// Maximum number of rows to return
    #[schema(example = 100)]
    pub limit: Option<usize>,
}

/// Response structure for query results
#[derive(Debug, Serialize, ToSchema)]
pub struct QueryResponse {
    /// Whether the query was successful
    pub success: bool,
    /// Query result data (if successful)
    pub data: Option<serde_json::Value>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Unique identifier for this query
    #[schema(example = "123e4567-e89b-12d3-a456-426614174000")]
    pub query_id: String,
    /// Query execution time in milliseconds
    #[schema(example = 42)]
    pub execution_time_ms: u64,
}

/// Response structure for health check endpoint
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    /// Server status
    #[schema(example = "healthy")]
    pub status: String,
    /// Unix timestamp
    #[schema(example = 1698765432)]
    pub timestamp: u64,
    /// Database file path (if using file database)
    #[schema(example = "/path/to/database.duckdb")]
    pub database_path: Option<String>,
    /// Whether database is in read-only mode
    pub readonly_mode: bool,
}
