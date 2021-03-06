use env_logger;
use hass_rs::client;
use lazy_static::lazy_static;
use std::env::var;

lazy_static! {
    static ref TOKEN: String =
        var("HASS_TOKEN").expect("please set up the HASS_TOKEN env variable before running this");

    static ref HOST: String = var("HASS_HOST").unwrap_or_else(|_| "localhost".to_string());
    static ref PORT: u16 = var("HASS_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8123);
}

//#[cfg_attr(feature = "tokio-runtime", tokio::main)]
#[cfg_attr(feature = "async-std-runtime", async_std::main)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("Creating the Websocket Client and Authenticate the session");
    let mut client = client::connect(&*HOST, *PORT, *PORT == 443).await?;

    client.auth_with_longlivedtoken(&*TOKEN).await?;
    println!("WebSocket connection and authentication works");

    println!("Get Hass Config");
    match client.get_config().await {
        Ok(v) => println!("{:?}", v),
        Err(err) => println!("Oh no, an error: {}", err),
    }

    // tokio::time::delay_for(std::time::Duration::from_secs(2)).await;
    async_std::task::sleep(std::time::Duration::from_secs(2)).await;

    // You could iterate and find the fields in your interest
    println!("Get Hass States");
    match client.get_states().await {
        Ok(v) => println!("{:?}", v),
        Err(err) => println!("Oh no, an error: {}", err),
    }

    // tokio::time::delay_for(std::time::Duration::from_secs(2)).await;
    async_std::task::sleep(std::time::Duration::from_secs(2)).await;

    // You could iterate and find the fields in your interest
    println!("Get Hass Services");
    match client.get_services().await {
        Ok(v) => println!("{:?}", v),
        Err(err) => println!("Oh no, an error: {}", err),
    }

    Ok(())
}
