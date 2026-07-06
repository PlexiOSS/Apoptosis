use sqlx::postgres::PgPoolOptions;

pub(crate) mod service;
pub mod entity;
pub mod types;
mod syscall;
mod migrations;
pub mod utils;
pub mod config;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(serde::Deserialize)]
struct BaseConfig {
    postgres_url: String,
    max_db_connections: u32,
}

#[tokio::main]
async fn main() {
    // Read config.json
    let config_data = std::fs::read_to_string("config.json").expect("Failed to read config.json");
    let config: serde_json::Value = serde_json::from_str(&config_data).expect("Failed to parse config.json");
    
    let base_config: BaseConfig = serde_json::from_value(config["base"].clone()).expect("Failed to deserialize base config");

    env_logger::init();
    log::info!("Postgres URL: {}", base_config.postgres_url);
    
    let pool = PgPoolOptions::new()
        .max_connections(base_config.max_db_connections)
        .connect(&base_config.postgres_url)
        .await
        .expect("Could not initialize connection");

    // Load up SampleLayer
    /*let th = layers::sample::samplelayer::SampleLayer::load(NewLayerOpts {
        config: serde_json::from_value(config["sample"].clone()).expect("Failed to deserialize config"),
        pool,
    });

    th.dispatch(SampleLayerEvent::default()).await.expect("Failed to dispatch event");*/
}
