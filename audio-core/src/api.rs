use std::sync::Arc;
use tokio::sync::Mutex;
use axum::{
    extract::{State, WebSocketUpgrade},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use futures_util::StreamExt;

use crate::device::DeviceManager;
use crate::audio::AudioEngine;

#[derive(Clone)]
pub struct AppState {
    pub device_mgr: Arc<Mutex<DeviceManager>>,
    pub audio_engine: Arc<Mutex<AudioEngine>>,
}

#[derive(Serialize)]
pub struct DeviceListResponse {
    pub mics: Vec<DeviceInfo>,
    pub loopbacks: Vec<DeviceInfo>,
    pub outputs: Vec<DeviceInfo>,
}

#[derive(Serialize, Clone)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct StartRequest {
    pub mic_device: String,
    pub loopback_device: String,
    pub virtual_mic_device: String,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub is_running: bool,
}

pub async fn list_devices(State(state): State<AppState>) -> Json<DeviceListResponse> {
    let mgr = state.device_mgr.lock().await;
    let mics = mgr.mic_devices().iter().map(|d| DeviceInfo {
        id: d.id.clone(),
        name: d.name.clone(),
    }).collect();
    let loopbacks = mgr.loopback_devices().iter().map(|d| DeviceInfo {
        id: d.id.clone(),
        name: d.name.clone(),
    }).collect();
    let outputs = mgr.output_devices().iter().map(|d| DeviceInfo {
        id: d.id.clone(),
        name: d.name.clone(),
    }).collect();

    Json(DeviceListResponse { mics, loopbacks, outputs })
}

pub async fn get_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let engine = state.audio_engine.lock().await;
    Json(StatusResponse {
        is_running: engine.is_running(),
    })
}

pub async fn start_processing(
    State(state): State<AppState>,
    Json(req): Json<StartRequest>,
) -> impl IntoResponse {
    let mut engine = state.audio_engine.lock().await;
    match engine.start(&req.mic_device, &req.loopback_device, &req.virtual_mic_device) {
        Ok(_) => (axum::http::StatusCode::OK, "Started"),
        Err(e) => {
            tracing::error!("Failed to start: {}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Failed")
        }
    }
}

pub async fn stop_processing(State(state): State<AppState>) -> impl IntoResponse {
    let mut engine = state.audio_engine.lock().await;
    engine.stop();
    (axum::http::StatusCode::OK, "Stopped")
}

pub async fn ws_handler(ws: WebSocketUpgrade, State(_state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| async move {
        let (_sender, mut receiver) = socket.split();
        
        while let Some(msg) = receiver.next().await {
            if let Ok(msg) = msg {
                if let Ok(text) = msg.into_text() {
                    tracing::info!("WS received: {}", text);
                }
            }
        }
    })
}
