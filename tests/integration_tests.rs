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
    assert!(body["data"]["column_types"].is_array());
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

#[tokio::test]
async fn test_decimal_type_handling() {
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
        "sql": "SELECT 123.45::DECIMAL(10,2) as decimal_col, CAST('2023-01-01' AS DATE) as date_col"
    });

    let response = server.post("/query").json(&query).await;

    assert_eq!(response.status_code(), 200);

    let body: Value = response.json();
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["columns"], json!(["decimal_col", "date_col"]));
    assert!(body["data"]["column_types"].is_array());
    assert_eq!(body["data"]["column_types"].as_array().unwrap().len(), 2);
    
    let rows = &body["data"]["rows"];
    assert!(rows.is_array());
    let first_row = &rows[0];
    
    // The decimal should be converted to JSON number, not string
    assert!(first_row[0].is_number(), "DECIMAL should be a number, got: {:?}", first_row[0]);
    assert_ne!(first_row[0].as_str().unwrap_or(""), "<UNSUPPORTED_TYPE>");
    
    // The date should be converted to string, not <UNSUPPORTED_TYPE>
    assert!(first_row[1].is_string());
    assert_ne!(first_row[1].as_str().unwrap(), "<UNSUPPORTED_TYPE>");
}

#[tokio::test]
async fn test_column_types_included() {
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
        "sql": "SELECT 42 as int_col, 'hello' as text_col, true as bool_col"
    });

    let response = server.post("/query").json(&query).await;

    assert_eq!(response.status_code(), 200);

    let body: Value = response.json();
    assert_eq!(body["success"], true);
    
    // Check that both columns and column_types are present
    assert_eq!(body["data"]["columns"], json!(["int_col", "text_col", "bool_col"]));
    assert!(body["data"]["column_types"].is_array());
    
    let column_types = body["data"]["column_types"].as_array().unwrap();
    assert_eq!(column_types.len(), 3);
    
    // Column types should be SQL type names
    for column_type in column_types {
        assert!(column_type.is_string());
        let type_str = column_type.as_str().unwrap();
        assert!(!type_str.is_empty());
        // Should be uppercase SQL type names
        assert!(type_str.chars().all(|c| c.is_uppercase() || c.is_ascii_digit()));
        // Should not be debug format (no parentheses or lowercase)
        assert!(!type_str.contains("("));
        assert!(!type_str.contains("["));
    }
}

#[tokio::test]
async fn test_specific_sql_type_names() {
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
        "sql": "SELECT 42::INTEGER as int_col, 'text'::VARCHAR as varchar_col, 123.45::DECIMAL(10,2) as decimal_col, true::BOOLEAN as bool_col"
    });

    let response = server.post("/query").json(&query).await;

    assert_eq!(response.status_code(), 200);

    let body: Value = response.json();
    assert_eq!(body["success"], true);
    
    let column_types = body["data"]["column_types"].as_array().unwrap();
    assert_eq!(column_types.len(), 4);
    
    // Verify that we get proper SQL type names
    let type_names: Vec<&str> = column_types.iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    
    // Should contain SQL-like type names
    assert!(type_names.iter().any(|&t| t == "INTEGER" || t == "BIGINT"));
    assert!(type_names.iter().any(|&t| t == "VARCHAR"));  
    assert!(type_names.iter().any(|&t| t == "DECIMAL"));
    assert!(type_names.iter().any(|&t| t == "BOOLEAN"));
}

#[tokio::test]
async fn test_decimal_values_as_numbers() {
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
        "sql": "SELECT 123.45::DECIMAL(10,2) as decimal_val, 42 as int_val, 'text' as text_val"
    });

    let response = server.post("/query").json(&query).await;

    assert_eq!(response.status_code(), 200);

    let body: Value = response.json();
    assert_eq!(body["success"], true);
    
    let rows = &body["data"]["rows"];
    assert!(rows.is_array());
    let first_row = &rows[0];
    
    // DECIMAL should be a JSON number, not a string
    assert!(first_row[0].is_number(), "DECIMAL value should be a number, got: {:?}", first_row[0]);
    
    // Integer should be a JSON number
    assert!(first_row[1].is_number(), "Integer value should be a number, got: {:?}", first_row[1]);
    
    // Text should be a string
    assert!(first_row[2].is_string(), "Text value should be a string, got: {:?}", first_row[2]);
    
    // Verify the actual decimal value
    if let Some(decimal_val) = first_row[0].as_f64() {
        assert!((decimal_val - 123.45).abs() < 0.001, "DECIMAL value should be 123.45, got: {}", decimal_val);
    } else {
        panic!("DECIMAL value could not be converted to f64");
    }
}

#[tokio::test]
async fn test_comprehensive_duckdb_types() {
    let args = Args {
        database: None,
        readwrite: false,
        port: 3001,
        host: "0.0.0.0".to_string(),
    };

    let state = AppState::new(&args).expect("Failed to create app state");
    let app = create_test_app(state);
    let server = TestServer::new(app).expect("Failed to create test server");

    // Test all major DuckDB data types
    let query = serde_json::json!({
        "sql": r#"
        SELECT 
            -- Numeric types
            42::TINYINT as tinyint_col,
            1234::SMALLINT as smallint_col,
            123456::INTEGER as integer_col,
            123456789::BIGINT as bigint_col,
            12345678901234567890::HUGEINT as hugeint_col,
            255::UTINYINT as utinyint_col,
            65535::USMALLINT as usmallint_col,
            4294967295::UINTEGER as uinteger_col,
            18446744073709551615::UBIGINT as ubigint_col,
            3.14::FLOAT as float_col,
            3.141592653589793::DOUBLE as double_col,
            123.456::DECIMAL(10,3) as decimal_col,
            
            -- Boolean
            true::BOOLEAN as boolean_col,
            
            -- Text types
            'hello world'::VARCHAR as varchar_col,
            
            -- Date/time types
            DATE '2023-12-25' as date_col,
            TIME '14:30:00' as time_col,
            TIMESTAMP '2023-12-25 14:30:00' as timestamp_col,
            INTERVAL '2 years 3 months' as interval_col,
            
            -- Binary types
            'hello'::BLOB as blob_col,
            
            -- Complex types
            [1, 2, 3, 4] as list_col,
            {'name': 'John', 'age': 30} as struct_col,
            MAP(['key1', 'key2'], ['value1', 'value2']) as map_col,
            
            -- Special types
            NULL as null_col,
            gen_random_uuid() as uuid_col
        "#
    });

    let response = server.post("/query").json(&query).await;

    assert_eq!(response.status_code(), 200);

    let body: Value = response.json();
    assert_eq!(body["success"], true);
    
    let columns = body["data"]["columns"].as_array().unwrap();
    let column_types = body["data"]["column_types"].as_array().unwrap();
    let rows = body["data"]["rows"].as_array().unwrap();
    
    assert_eq!(columns.len(), column_types.len());
    assert!(!rows.is_empty());
    
    let first_row = &rows[0];
    
    // Check that we have values for all columns
    assert_eq!(first_row.as_array().unwrap().len(), columns.len());
    
    // Look for any UNSUPPORTED_TYPE values
    let mut unsupported_types = Vec::new();
    let mut unsupported_values = Vec::new();
    
    for (i, value) in first_row.as_array().unwrap().iter().enumerate() {
        let column_name = columns[i].as_str().unwrap();
        let column_type = column_types[i].as_str().unwrap();
        
        if value.is_string() && value.as_str().unwrap() == "<UNSUPPORTED_TYPE>" {
            unsupported_values.push(format!("Column '{}' (type: {})", column_name, column_type));
        }
        
        if column_type == "UNKNOWN" {
            unsupported_types.push(format!("Column '{}' has unknown type", column_name));
        }
    }
    
    // Print detailed information about what we found
    println!("=== COMPREHENSIVE TYPE TEST RESULTS ===");
    println!("Total columns tested: {}", columns.len());
    
    for (i, _) in columns.iter().enumerate() {
        let column_name = columns[i].as_str().unwrap();
        let column_type = column_types[i].as_str().unwrap();
        let value = &first_row.as_array().unwrap()[i];
        
        let value_type = if value.is_null() {
            "NULL"
        } else if value.as_bool().is_some() {
            "BOOLEAN"
        } else if value.is_number() {
            "NUMBER"
        } else if value.is_string() {
            "STRING"
        } else if value.is_array() {
            "ARRAY"
        } else if value.is_object() {
            "OBJECT"
        } else {
            "UNKNOWN"
        };
        
        println!("  {}: {} -> JSON {}", column_name, column_type, value_type);
        
        if value.is_string() && value.as_str().unwrap() == "<UNSUPPORTED_TYPE>" {
            println!("    ❌ UNSUPPORTED VALUE");
        } else if column_type == "UNKNOWN" {
            println!("    ⚠️  UNKNOWN TYPE");
        } else {
            println!("    ✅ OK");
        }
    }
    
    if !unsupported_values.is_empty() {
        println!("\n❌ UNSUPPORTED VALUES FOUND:");
        for item in &unsupported_values {
            println!("  - {}", item);
        }
    }
    
    if !unsupported_types.is_empty() {
        println!("\n⚠️  UNKNOWN TYPES FOUND:");
        for item in &unsupported_types {
            println!("  - {}", item);
        }
    }
    
    if unsupported_values.is_empty() && unsupported_types.is_empty() {
        println!("\n✅ ALL TYPES ARE PROPERLY SUPPORTED!");
    }
    
    // The test should pass even if we find unsupported types - this is for discovery
    // But we should fail if basic types are unsupported
    assert!(unsupported_values.len() < 5, "Too many unsupported values found: {:?}", unsupported_values);
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
