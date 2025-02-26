use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::oneshot;

use metaproxy::proxy::{BindingMap, ProxyBinding};

#[tokio::test]
async fn test_proxy_binding_creation() {
    // Create a binding map
    let bindings: BindingMap = Arc::new(Mutex::new(HashMap::new()));
    
    // Create a shutdown channel
    let (shutdown_tx, _) = oneshot::channel();
    
    // Create a proxy binding
    let upstream = Arc::new(Mutex::new("http://127.0.0.1:8080".to_string()));
    let binding = ProxyBinding {
        port: 9000,
        upstream: upstream.clone(),
        shutdown_tx,
    };
    
    // Add the binding to the map
    {
        let mut bindings_lock = bindings.lock().await;
        bindings_lock.insert(9000, binding);
    }
    
    // Verify the binding exists
    {
        let bindings_lock = bindings.lock().await;
        assert!(bindings_lock.contains_key(&9000));
        
        // Check the upstream value
        let binding = bindings_lock.get(&9000).unwrap();
        let upstream_value = binding.upstream.lock().await;
        assert_eq!(*upstream_value, "http://127.0.0.1:8080");
    }
    
    // Update the upstream
    {
        let bindings_lock = bindings.lock().await;
        let binding = bindings_lock.get(&9000).unwrap();
        let mut upstream_value = binding.upstream.lock().await;
        *upstream_value = "http://127.0.0.1:9090".to_string();
    }
    
    // Verify the update
    {
        let bindings_lock = bindings.lock().await;
        let binding = bindings_lock.get(&9000).unwrap();
        let upstream_value = binding.upstream.lock().await;
        assert_eq!(*upstream_value, "http://127.0.0.1:9090");
    }
}

// Note: Testing the actual proxy functionality would require setting up mock TCP servers
// which is beyond the scope of these basic tests. In a real-world scenario, we would
// use tools like mockito or wiremock to simulate HTTP servers.
