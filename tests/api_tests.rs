use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use warp::http::StatusCode;
use warp::test::request;

use metaproxy::api;
use metaproxy::proxy::{BindingMap, ProxyBinding};

#[tokio::test]
async fn test_health_endpoint() {
    // Create an empty binding map
    let bindings: BindingMap = Arc::new(Mutex::new(HashMap::new()));
    
    // Create the API routes
    let routes = api::create_routes(bindings.clone());
    
    // Test the health endpoint
    let resp = request()
        .method("GET")
        .path("/health")
        .reply(&routes)
        .await;
    
    assert_eq!(resp.status(), StatusCode::OK);
    
    // Parse the response body
    let body = String::from_utf8(resp.body().to_vec()).unwrap();
    assert!(body.contains("\"status\":\"ok\""));
    assert!(body.contains("\"bindings\":[]"));
}

#[tokio::test]
async fn test_create_proxy_binding() {
    // Create an empty binding map
    let bindings: BindingMap = Arc::new(Mutex::new(HashMap::new()));
    
    // Create the API routes
    let routes = api::create_routes(bindings.clone());
    
    // Test creating a new proxy binding
    let resp = request()
        .method("POST")
        .path("/proxy")
        .json(&serde_json::json!({
            "port": 9000,
            "upstream": "http://127.0.0.1:8080"
        }))
        .reply(&routes)
        .await;
    
    assert_eq!(resp.status(), StatusCode::OK);
    
    // Parse the response body
    let body = String::from_utf8(resp.body().to_vec()).unwrap();
    assert!(body.contains("\"status\":\"created\""));
    assert!(body.contains("\"port\":9000"));
    
    // Check that the binding was created in the map
    let bindings_lock = bindings.lock().await;
    assert!(bindings_lock.contains_key(&9000));
    
    // Check the upstream value
    let binding = bindings_lock.get(&9000).unwrap();
    let upstream = binding.upstream.lock().await;
    assert_eq!(*upstream, "http://127.0.0.1:8080");
}

// Note: In a real test, we would need to mock the TCP listener creation
// since we can't actually bind to ports during tests without potential conflicts.
// For now, we'll focus on testing the API endpoints only.
