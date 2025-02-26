use metaproxy::config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let config = Config::from_args();
    
    // Run the proxy server
    metaproxy::run(config).await?;
    
    Ok(())
}
