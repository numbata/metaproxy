/*!
 * # API Module
 *
 * This module provides the REST API for managing proxy bindings.
 * It defines routes for creating, updating, and deleting proxy bindings,
 * as well as a health check endpoint.
 */

use std::convert::Infallible;
use warp::{Filter, Reply, Rejection};
use serde_json::{json, Value};
use tokio::sync::oneshot;
use log::{info, warn, error, debug};
use crate::proxy::{BindingMap, ProxyBinding, spawn_proxy_listener};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::error::{Error, CustomRejection};

/// Create API routes for the proxy server
///
/// This function sets up all the API routes for the proxy server,
/// including routes for managing proxy bindings and a health check endpoint.
///
/// # Arguments
///
/// * `bindings` - Shared state containing active proxy bindings
///
/// # Returns
///
/// A warp filter that handles all API routes
pub fn create_routes(
    bindings: BindingMap,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let proxy_routes = create_proxy_routes(bindings.clone());
    let health_route = create_health_route(bindings.clone());
    
    proxy_routes.or(health_route)
}

/// Create routes for managing proxy bindings
///
/// This function sets up routes for creating, updating, and deleting proxy bindings.
/// It handles POST, PUT, and DELETE requests to the `/proxy` endpoint.
///
/// # Arguments
///
/// * `bindings` - Shared state containing active proxy bindings
///
/// # Returns
///
/// A warp filter that handles proxy binding management routes
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
///
/// This function sets up a route for checking the health of the proxy server.
/// It returns information about the server status and active bindings.
///
/// # Arguments
///
/// * `bindings` - Shared state containing active proxy bindings
///
/// # Returns
///
/// A warp filter that handles health check requests
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
///
/// This function handles requests for creating, updating, and deleting proxy bindings.
/// It processes the request based on the HTTP method and updates the shared state accordingly.
///
/// # Arguments
///
/// * `port` - The port number for the proxy binding (0 for POST requests)
/// * `method` - The HTTP method (POST, PUT, or DELETE)
/// * `body` - The request body as JSON
/// * `bindings` - Shared state containing active proxy bindings
///
/// # Returns
///
/// A result containing a JSON response or a rejection
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

            info!("Creating new proxy binding on port {} with upstream {}", new_port, upstream);

            // Get the lock once for the entire operation
            let mut bindings_lock = bindings.lock().await;
            
            // Check if the binding already exists and return error if it does
            if let Some(_) = bindings_lock.get(&new_port) {
                warn!("Binding on port {} already exists", new_port);
                return Err(warp::reject::custom(CustomRejection(Error::Custom(format!("Binding on port {} already exists", new_port)))));
            }

            // Create a new binding.
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            let upstream_arc = Arc::new(Mutex::new(upstream.clone()));
            
            // Spawn a new proxy listener.
            let upstream_clone = upstream_arc.clone();
            tokio::spawn(async move {
                if let Err(e) = spawn_proxy_listener(new_port, upstream_clone, shutdown_rx).await {
                    error!("Error in proxy listener: {}", e);
                }
            });

            // Store the binding.
            bindings_lock.insert(new_port, ProxyBinding {
                port: new_port,
                upstream: upstream_arc,
                shutdown_tx,
            });

            debug!("Added binding for port {} to binding map", new_port);

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
                warn!("Missing port in path for PUT request");
                return Err(warp::reject::custom(CustomRejection(Error::Custom("Missing port in path".into()))));
            }
            
            // Extract the new upstream from the JSON body.
            let new_upstream = body.get("upstream")
                .and_then(|v| v.as_str())
                .ok_or_else(|| warp::reject::custom(CustomRejection(Error::Custom("Missing upstream".into()))))?
                .to_string();

            info!("Updating proxy binding on port {} with new upstream {}", port, new_upstream);

            // Get the lock once for the entire operation
            let bindings_lock = bindings.lock().await;
            
            // Check if the binding exists.
            if let Some(binding) = bindings_lock.get(&port) {
                // Update the upstream.
                let mut upstream_lock = binding.upstream.lock().await;
                *upstream_lock = new_upstream.clone();
                
                debug!("Updated upstream for port {} to {}", port, new_upstream);
                
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
                warn!("No binding found for port {} during update", port);
                Err(warp::reject::custom(CustomRejection(Error::Custom(format!("No binding found for port {}", port)))))
            }
        }
        warp::http::Method::DELETE => {
            // For deletion, use the path parameter as the port.
            if port == 0 {
                warn!("Missing port in path for DELETE request");
                return Err(warp::reject::custom(CustomRejection(Error::Custom("Missing port in path".into()))));
            }
            
            info!("Deleting proxy binding on port {}", port);
            
            // Get the lock once for the entire operation
            let mut bindings_lock = bindings.lock().await;
            
            // Check if the binding exists and remove it
            if let Some(binding) = bindings_lock.remove(&port) {
                // Signal the listener to shut down.
                let _ = binding.shutdown_tx.send(());
                debug!("Sent shutdown signal to proxy listener on port {}", port);
                
                // Drop the bindings lock before returning
                drop(bindings_lock);
                
                Ok(warp::reply::json(&json!({
                    "status": "deleted",
                    "port": port
                })))
            } else {
                warn!("No binding found for port {} during deletion", port);
                Err(warp::reject::custom(CustomRejection(Error::Custom(format!("No binding found for port {}", port)))))
            }
        }
        _ => {
            // Method not allowed
            warn!("Method not allowed: {}", method);
            Err(warp::reject::custom(CustomRejection(Error::Custom("Method not allowed".into()))))
        }
    }
}

/// Handle health check requests
///
/// This function handles requests to the health check endpoint.
/// It returns information about the server status and active bindings.
///
/// # Arguments
///
/// * `bindings` - Shared state containing active proxy bindings
///
/// # Returns
///
/// A result containing a JSON response
async fn handle_health_request(
    bindings: BindingMap,
) -> std::result::Result<impl Reply, Infallible> {
    debug!("Received health check request");
    
    let bindings_lock = bindings.lock().await;
    let binding_count = bindings_lock.len();
    
    let binding_info: Vec<Value> = bindings_lock.iter().map(|(port, binding)| {
        let upstream = binding.upstream.try_lock().map(|u| u.clone()).unwrap_or_else(|_| "locked".to_string());
        json!({
            "port": port,
            "upstream": upstream
        })
    }).collect();
    
    drop(bindings_lock);
    
    debug!("Health check found {} active bindings", binding_count);
    
    Ok(warp::reply::json(&json!({
        "status": "ok",
        "active_bindings": binding_count,
        "bindings": binding_info
    })))
}
