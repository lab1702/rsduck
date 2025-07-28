use axum::{
    routing::{get, post},
    Router,
};
use clap::Parser;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use rsduck::{Args, AppState, health_check, execute_query_post, execute_query_get, execute_command_post, execute_command_get};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rsduck=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    let state = AppState::new(&args)?;

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/query", post(execute_query_post))
        .route("/query", get(execute_query_get))
        .route("/execute", post(execute_command_post))
        .route("/execute", get(execute_command_get))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let bind_addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    
    tracing::info!("DuckDB REST server starting on http://{}", bind_addr);
    tracing::info!("Available endpoints:");
    tracing::info!("  GET  /health - Health check");
    tracing::info!("  POST /query  - Execute SQL query that returns data (JSON body)");
    tracing::info!("  GET  /query?sql=<query> - Execute SQL query that returns data (URL parameter)");
    tracing::info!("  POST /execute - Execute SQL command (CREATE, INSERT, etc.) (JSON body)");
    tracing::info!("  GET  /execute?sql=<command> - Execute SQL command (CREATE, INSERT, etc.) (URL parameter)");
    tracing::info!("Usage examples:");
    tracing::info!("  cargo run                                    # In-memory database");
    tracing::info!("  cargo run -- --database mydb.duckdb         # Read-only file");
    tracing::info!("  cargo run -- --database mydb.duckdb --readwrite  # Read-write file");
    tracing::info!("  cargo run -- --port 8080                    # Custom port");

    axum::serve(listener, app).await?;

    Ok(())
}
