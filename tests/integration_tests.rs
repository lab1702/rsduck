use axum_test::TestServer;
use rsduck::{AppState, Args};
use serde_json::{Value, json};

#[tokio::test]
async fn test_health_check() {
    let args = Args {
        database: None,
        readwrite: false,
        port: 3001,
        host: "0.0.0.0".to_string(),
    };

    let state = AppState::new(&args).expect("Failed to create app state");
    let app = create_test_app(state);
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/health").await;

    assert_eq!(response.status_code(), 200);

    let body: Value = response.json();
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["readonly_mode"], false);
    assert!(body["timestamp"].is_number());
}

#[tokio::test]
async fn test_simple_query() {
    let args = Args {
        database: None,
        readwrite: false,
        port: 3001,
        host: "0.0.0.0".to_string(),
    };

    let state = AppState::new(&args).expect("Failed to create app state");
    let app = create_test_app(state);
    let server = TestServer::new(app).expect("Failed to create test server");

    let query = serde_json::json!({
        "sql": "SELECT 1 as test_col"
    });

    let response = server.post("/query").json(&query).await;

    assert_eq!(response.status_code(), 200);

    let body: Value = response.json();
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["columns"], json!(["test_col"]));
    assert_eq!(body["data"]["rows"], json!([[1]]));
    assert_eq!(body["data"]["row_count"], 1);
}

#[tokio::test]
async fn test_query_with_limit() {
    let args = Args {
        database: None,
        readwrite: false,
        port: 3001,
        host: "0.0.0.0".to_string(),
    };

    let state = AppState::new(&args).expect("Failed to create app state");
    let app = create_test_app(state);
    let server = TestServer::new(app).expect("Failed to create test server");

    let query = serde_json::json!({
        "sql": "SELECT * FROM (VALUES (1), (2), (3), (4), (5)) AS t(x)",
        "limit": 3
    });

    let response = server.post("/query").json(&query).await;

    assert_eq!(response.status_code(), 200);

    let body: Value = response.json();
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["row_count"], 3);
    assert_eq!(body["data"]["limit_applied"], 3);
}

#[tokio::test]
async fn test_readonly_protection() {
    let args = Args {
        database: None,
        readwrite: false, // Force readonly mode
        port: 3001,
        host: "0.0.0.0".to_string(),
    };

    // Create a custom state with readonly forced
    let state = AppState {
        pool: AppState::new(&args).unwrap().pool,
        db_path: None,
        is_readonly: true, // Force readonly
    };

    let app = create_test_app(state);
    let server = TestServer::new(app).expect("Failed to create test server");

    let query = serde_json::json!({
        "sql": "CREATE TABLE test (id INT)"
    });

    let response = server.post("/query").json(&query).await;

    assert_eq!(response.status_code(), 403);

    let body: Value = response.json();
    assert_eq!(body["success"], false);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("read-only mode")
    );
}

#[tokio::test]
async fn test_sql_injection_protection() {
    let args = Args {
        database: None,
        readwrite: false,
        port: 3001,
        host: "0.0.0.0".to_string(),
    };

    // Create a custom state with readonly forced
    let state = AppState {
        pool: AppState::new(&args).unwrap().pool,
        db_path: None,
        is_readonly: true, // Force readonly
    };

    let app = create_test_app(state);
    let server = TestServer::new(app).expect("Failed to create test server");

    // Test comment-based bypass attempt
    let query = serde_json::json!({
        "sql": "/* comment */ INSERT INTO test VALUES (1)"
    });

    let response = server.post("/query").json(&query).await;

    assert_eq!(response.status_code(), 403);

    let body: Value = response.json();
    assert_eq!(body["success"], false);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("read-only mode")
    );
}

#[tokio::test]
async fn test_missing_sql_parameter() {
    let args = Args {
        database: None,
        readwrite: false,
        port: 3001,
        host: "0.0.0.0".to_string(),
    };

    let state = AppState::new(&args).expect("Failed to create app state");
    let app = create_test_app(state);
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/query").await;

    assert_eq!(response.status_code(), 400);

    let body: Value = response.json();
    assert_eq!(body["success"], false);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Missing 'sql' parameter")
    );
}

fn create_test_app(state: AppState) -> axum::Router {
    use axum::routing::{get, post};
    use rsduck::{
        execute_command_get, execute_command_post, execute_query_get, execute_query_post,
        health_check,
    };

    axum::Router::new()
        .route("/health", get(health_check))
        .route("/query", post(execute_query_post))
        .route("/query", get(execute_query_get))
        .route("/execute", post(execute_command_post))
        .route("/execute", get(execute_command_get))
        .with_state(state)
}
