#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;
mod device;

use config::SavedConfig;
use device::DeviceManager;
use audio::AudioEngine;
use serde::Serialize;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

pub struct AppState {
    pub device_mgr: DeviceManager,
    pub audio_engine: AudioEngine,
}

#[derive(Serialize, Clone)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct DeviceListResponse {
    pub mics: Vec<DeviceInfo>,
    pub loopbacks: Vec<DeviceInfo>,
    pub outputs: Vec<DeviceInfo>,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub is_running: bool,
}

#[tauri::command]
async fn list_devices(
    state: tauri::State<'_, Arc<tauri::async_runtime::Mutex<AppState>>>,
) -> Result<DeviceListResponse, String> {
    let state = state.lock().await;
    let mics = state.device_mgr.mic_devices().iter().map(|d| DeviceInfo {
        id: d.id.clone(),
        name: d.name.clone(),
    }).collect();
    let loopbacks = state.device_mgr.loopback_devices().iter().map(|d| DeviceInfo {
        id: d.id.clone(),
        name: d.name.clone(),
    }).collect();
    let outputs = state.device_mgr.output_devices().iter().map(|d| DeviceInfo {
        id: d.id.clone(),
        name: d.name.clone(),
    }).collect();
    Ok(DeviceListResponse { mics, loopbacks, outputs })
}

#[tauri::command]
async fn get_status(
    state: tauri::State<'_, Arc<tauri::async_runtime::Mutex<AppState>>>,
) -> Result<StatusResponse, String> {
    let state = state.lock().await;
    Ok(StatusResponse {
        is_running: state.audio_engine.is_running(),
    })
}

#[tauri::command]
async fn start_processing(
    mic_device: String,
    loopback_device: String,
    virtual_mic_device: String,
    state: tauri::State<'_, Arc<tauri::async_runtime::Mutex<AppState>>>,
) -> Result<(), String> {
    let mut state = state.lock().await;
    state
        .audio_engine
        .start(&mic_device, &loopback_device, &virtual_mic_device)
        .map_err(|e| e.to_string())?;

    let cfg = SavedConfig {
        mic_device,
        loopback_device,
        virtual_mic_device,
    };
    if let Err(e) = cfg.save() {
        tracing::warn!("Failed to save config: {}", e);
    }
    Ok(())
}

#[tauri::command]
async fn stop_processing(
    state: tauri::State<'_, Arc<tauri::async_runtime::Mutex<AppState>>>,
) -> Result<(), String> {
    let mut state = state.lock().await;
    state.audio_engine.stop();
    Ok(())
}

#[tauri::command]
async fn get_config() -> Result<SavedConfig, String> {
    Ok(SavedConfig::load())
}

/// 未修改的 VB-CABLE 官方安装程序（VB-Audio 许可允许原样捆绑分发）
static VBCABLE_SETUP: &[u8] = include_bytes!("../resources/vbcable/VBCABLE_Setup_x64.exe");

#[tauri::command]
async fn install_vbcable() -> Result<String, String> {
    let temp = std::env::temp_dir().join("VBCABLE_Setup_x64.exe");
    std::fs::write(&temp, VBCABLE_SETUP).map_err(|e| e.to_string())?;

    // 安装程序自身请求管理员权限（UAC 提示中会显示 VB-Audio 品牌，满足分发条款）
    std::process::Command::new(&temp)
        .spawn()
        .map_err(|e| format!("启动安装程序失败: {}", e))?;

    Ok("VB-CABLE 安装程序已启动，请按提示点击 Install Driver，完成后建议重启电脑".to_string())
}

#[tauri::command]
async fn refresh_devices(
    state: tauri::State<'_, Arc<tauri::async_runtime::Mutex<AppState>>>,
) -> Result<DeviceListResponse, String> {
    let new_mgr = DeviceManager::new().map_err(|e| e.to_string())?;
    let mut state = state.lock().await;
    state.device_mgr = new_mgr;

    let mics = state.device_mgr.mic_devices().iter().map(|d| DeviceInfo {
        id: d.id.clone(),
        name: d.name.clone(),
    }).collect();
    let loopbacks = state.device_mgr.loopback_devices().iter().map(|d| DeviceInfo {
        id: d.id.clone(),
        name: d.name.clone(),
    }).collect();
    let outputs = state.device_mgr.output_devices().iter().map(|d| DeviceInfo {
        id: d.id.clone(),
        name: d.name.clone(),
    }).collect();
    Ok(DeviceListResponse { mics, loopbacks, outputs })
}

fn main() {
    tracing_subscriber::fmt::init();

    let device_mgr = DeviceManager::new().expect("Failed to enumerate audio devices");
    let audio_engine = AudioEngine::new();
    let state = Arc::new(tauri::async_runtime::Mutex::new(AppState {
        device_mgr,
        audio_engine,
    }));

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            list_devices,
            get_status,
            start_processing,
            stop_processing,
            get_config,
            install_vbcable,
            refresh_devices,
        ])
        .setup(|app| {
            let show_item = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Echo AEC")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
