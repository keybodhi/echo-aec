mod audio;
mod device;
mod api;

use std::sync::Arc;
use tokio::sync::Mutex;
use axum::{Router, routing::get};
use tower_http::cors::{CorsLayer, Any};

use audio::AudioEngine;
use device::DeviceManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let device_mgr = Arc::new(Mutex::new(DeviceManager::new()?));
    let audio_engine = Arc::new(Mutex::new(AudioEngine::new()));

    let app_state = api::AppState {
        device_mgr: device_mgr.clone(),
        audio_engine: audio_engine.clone(),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/devices", get(api::list_devices))
        .route("/api/status", get(api::get_status))
        .route("/api/start", axum::routing::post(api::start_processing))
        .route("/api/stop", axum::routing::post(api::stop_processing))
        .route("/ws", get(api::ws_handler))
        .layer(cors)
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    tracing::info!("Server: http://127.0.0.1:3000");
    axum::serve(listener, app).await?;

    Ok(())
}
