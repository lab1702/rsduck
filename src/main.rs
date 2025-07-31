use axum::{
    Router,
    routing::{get, post},
};
use clap::Parser;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use rsduck::{
    AppState, Args, HealthResponse, QueryParams, QueryRequest, QueryResponse, execute_command_get,
    execute_command_post, execute_query_get, execute_query_post, health_check,
};

#[derive(OpenApi)]
#[openapi(
    paths(
        rsduck::health_check,
        rsduck::execute_query_post,
        rsduck::execute_query_get,
        rsduck::execute_command_post,
        rsduck::execute_command_get
    ),
    components(
        schemas(QueryRequest, QueryResponse, HealthResponse, QueryParams)
    ),
    tags(
        (name = "health", description = "Health check endpoints"),
        (name = "query", description = "SQL query execution endpoints"),
        (name = "execute", description = "SQL command execution endpoints")
    ),
    info(
        title = "RSDuck - DuckDB REST API",
        description = "A REST API for executing SQL queries and commands against DuckDB",
        version = "1.0.0",
        contact(
            name = "RSDuck API",
            url = "https://github.com/your-org/rsduck"
        )
    )
)]
struct ApiDoc;

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
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
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
    tracing::info!("Swagger UI available at: http://{}/swagger-ui", bind_addr);
    tracing::info!("Available endpoints:");
    tracing::info!("  GET  /health - Health check");
    tracing::info!("  POST /query  - Execute SQL query that returns data (JSON body)");
    tracing::info!(
        "  GET  /query?sql=<query> - Execute SQL query that returns data (URL parameter)"
    );
    tracing::info!("  POST /execute - Execute SQL command (CREATE, INSERT, etc.) (JSON body)");
    tracing::info!(
        "  GET  /execute?sql=<command> - Execute SQL command (CREATE, INSERT, etc.) (URL parameter)"
    );
    tracing::info!("Usage examples:");
    tracing::info!("  cargo run                                    # In-memory database");
    tracing::info!("  cargo run -- --database mydb.duckdb         # Read-only file");
    tracing::info!("  cargo run -- --database mydb.duckdb --readwrite  # Read-write file");
    tracing::info!("  cargo run -- --port 8080                    # Custom port");
    tracing::info!("Press Ctrl+C to stop the server");

    // Set up graceful shutdown
    let server = axum::serve(listener, app);
    
    // Handle Ctrl+C for graceful shutdown on both Unix and Windows
    let shutdown_signal = async {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
            let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
            
            tokio::select! {
                _ = sigterm.recv() => {
                    tracing::info!("Received SIGTERM, shutting down gracefully...");
                }
                _ = sigint.recv() => {
                    tracing::info!("Received SIGINT (Ctrl+C), shutting down gracefully...");
                }
            }
        }
        
        #[cfg(windows)]
        {
            use tokio::signal::windows::{ctrl_c, ctrl_break, ctrl_close, ctrl_logoff, ctrl_shutdown};
            
            let mut ctrl_c = ctrl_c().expect("failed to install Ctrl+C handler");
            let mut ctrl_break = ctrl_break().expect("failed to install Ctrl+Break handler");
            let mut ctrl_close = ctrl_close().expect("failed to install Ctrl+Close handler");
            let mut ctrl_logoff = ctrl_logoff().expect("failed to install Ctrl+Logoff handler");
            let mut ctrl_shutdown = ctrl_shutdown().expect("failed to install Ctrl+Shutdown handler");
            
            tokio::select! {
                _ = ctrl_c.recv() => {
                    tracing::info!("Received Ctrl+C, shutting down gracefully...");
                }
                _ = ctrl_break.recv() => {
                    tracing::info!("Received Ctrl+Break, shutting down gracefully...");
                }
                _ = ctrl_close.recv() => {
                    tracing::info!("Received Ctrl+Close, shutting down gracefully...");
                }
                _ = ctrl_logoff.recv() => {
                    tracing::info!("Received Ctrl+Logoff, shutting down gracefully...");
                }
                _ = ctrl_shutdown.recv() => {
                    tracing::info!("Received Ctrl+Shutdown, shutting down gracefully...");
                }
            }
        }
        
        #[cfg(not(any(unix, windows)))]
        {
            // Fallback for other platforms
            tokio::signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
            tracing::info!("Received Ctrl+C, shutting down gracefully...");
        }
    };

    // Run server with graceful shutdown
    tokio::select! {
        result = server => {
            if let Err(err) = result {
                tracing::error!("Server error: {}", err);
                return Err(err.into());
            }
        }
        _ = shutdown_signal => {
            tracing::info!("Shutdown signal received, server is stopping...");
        }
    }

    Ok(())
}
