use metaproxy::config::Config;
use log::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the logger
    env_logger::init();
    
    // Parse command line arguments
    let config = Config::from_args();
    
    info!("Starting metaproxy with configuration: {:?}", config);
    
    // Run the proxy server
    metaproxy::run(config).await?;
    
    Ok(())
}
