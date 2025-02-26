use std::convert::Infallible;
use warp::{Filter, Reply, Rejection};
use serde_json::{json, Value};
use tokio::sync::oneshot;
use crate::proxy::{BindingMap, ProxyBinding, spawn_proxy_listener};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::error::{Error, CustomRejection};

/// Create API routes for the proxy server
pub fn create_routes(
    bindings: BindingMap,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let proxy_routes = create_proxy_routes(bindings.clone());
    let health_route = create_health_route(bindings.clone());
    
    proxy_routes.or(health_route)
}

/// Create routes for managing proxy bindings
fn create_proxy_routes(
    bindings: BindingMap,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let bindings_filter = warp::any().map(move || bindings.clone());
    
    // Route for POST requests (create)
    let post_route = warp::path("proxy")
        .and(warp::post())
        .and(warp::body::json())
        .and(bindings_filter.clone())
        .and_then(|body: Value, bindings: BindingMap| {
            handle_proxy_request(0, warp::http::Method::POST, body, bindings)
        });
    
    // Route for PUT requests (update)
    let put_route = warp::path("proxy")
        .and(warp::path::param::<u16>())
        .and(warp::put())
        .and(warp::body::json())
        .and(bindings_filter.clone())
        .and_then(|port: u16, body: Value, bindings: BindingMap| {
            handle_proxy_request(port, warp::http::Method::PUT, body, bindings)
        });
    
    // Route for DELETE requests (delete) - no JSON body required
    let delete_route = warp::path("proxy")
        .and(warp::path::param::<u16>())
        .and(warp::delete())
        .and(bindings_filter.clone())
        .and_then(|port: u16, bindings: BindingMap| {
            // Pass an empty JSON object for the body
            handle_proxy_request(port, warp::http::Method::DELETE, json!({}), bindings)
        });
    
    // Combine all routes
    post_route.or(put_route).or(delete_route)
}

/// Create health check route
fn create_health_route(
    bindings: BindingMap,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let bindings_filter = warp::any().map(move || bindings.clone());
    
    warp::path("health")
        .and(warp::get())
        .and(bindings_filter)
        .and_then(handle_health_request)
}

/// Handle proxy binding management requests
async fn handle_proxy_request(
    port: u16,
    method: warp::http::Method,
    body: Value,
    bindings: BindingMap,
) -> std::result::Result<impl Reply, Rejection> {
    match method {
        warp::http::Method::POST => {
            // For creation, extract "port" and "upstream" from the JSON body.
            let new_port = body.get("port")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| warp::reject::custom(CustomRejection(Error::Custom("Missing port".into()))))?
                as u16;
            let upstream = body.get("upstream")
                .and_then(|v| v.as_str())
                .ok_or_else(|| warp::reject::custom(CustomRejection(Error::Custom("Missing upstream".into()))))?
                .to_string();

            // Get the lock once for the entire operation
            let mut bindings_lock = bindings.lock().await;
            
            // Check if the binding already exists and return error if it does
            if let Some(_) = bindings_lock.get(&new_port) {
                return Err(warp::reject::custom(CustomRejection(Error::Custom(format!("Binding on port {} already exists", new_port)))));
            }

            // Create a new binding.
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            let upstream_arc = Arc::new(Mutex::new(upstream.clone()));
            
            // Spawn a new proxy listener.
            let upstream_clone = upstream_arc.clone();
            tokio::spawn(async move {
                if let Err(e) = spawn_proxy_listener(new_port, upstream_clone, shutdown_rx).await {
                    eprintln!("Error in proxy listener: {}", e);
                }
            });

            // Store the binding.
            bindings_lock.insert(new_port, ProxyBinding {
                port: new_port,
                upstream: upstream_arc,
                shutdown_tx,
            });

            // Drop the lock before returning
            drop(bindings_lock);

            Ok(warp::reply::json(&json!({
                "status": "created",
                "port": new_port,
                "upstream": upstream
            })))
        }
        warp::http::Method::PUT => {
            // For update, use the path parameter as the port.
            if port == 0 {
                return Err(warp::reject::custom(CustomRejection(Error::Custom("Missing port in path".into()))));
            }
            
            // Extract the new upstream from the JSON body.
            let new_upstream = body.get("upstream")
                .and_then(|v| v.as_str())
                .ok_or_else(|| warp::reject::custom(CustomRejection(Error::Custom("Missing upstream".into()))))?
                .to_string();

            // Get the lock once for the entire operation
            let bindings_lock = bindings.lock().await;
            
            // Check if the binding exists.
            if let Some(binding) = bindings_lock.get(&port) {
                // Update the upstream.
                let mut upstream_lock = binding.upstream.lock().await;
                *upstream_lock = new_upstream.clone();
                
                // Drop the upstream lock
                drop(upstream_lock);
                
                // Drop the bindings lock before returning
                drop(bindings_lock);
                
                Ok(warp::reply::json(&json!({
                    "status": "updated",
                    "port": port,
                    "upstream": new_upstream
                })))
            } else {
                Err(warp::reject::custom(CustomRejection(Error::Custom(format!("No binding found for port {}", port)))))
            }
        }
        warp::http::Method::DELETE => {
            // For deletion, use the path parameter as the port.
            if port == 0 {
                return Err(warp::reject::custom(CustomRejection(Error::Custom("Missing port in path".into()))));
            }
            
            // Get the lock once for the entire operation
            let mut bindings_lock = bindings.lock().await;
            
            // Check if the binding exists and remove it
            if let Some(binding) = bindings_lock.remove(&port) {
                // Signal the listener to shut down.
                let _ = binding.shutdown_tx.send(());
                
                // Drop the bindings lock before returning
                drop(bindings_lock);
                
                Ok(warp::reply::json(&json!({
                    "status": "deleted",
                    "port": port
                })))
            } else {
                Err(warp::reject::custom(CustomRejection(Error::Custom(format!("No binding found for port {}", port)))))
            }
        }
        _ => {
            // Method not allowed
            Err(warp::reject::custom(CustomRejection(Error::Custom("Method not allowed".into()))))
        }
    }
}

/// Handle health check requests
async fn handle_health_request(
    bindings: BindingMap,
) -> std::result::Result<impl Reply, Infallible> {
    let bindings_lock = bindings.lock().await;
    let mut bindings_json = Vec::new();

    for (port, binding) in bindings_lock.iter() {
        let upstream = binding.upstream.lock().await.clone();
        bindings_json.push(json!({
            "port": port,
            "upstream": upstream
        }));
    }

    Ok(warp::reply::json(&json!({
        "status": "ok",
        "bindings": bindings_json
    })))
}
