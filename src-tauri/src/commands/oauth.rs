use crate::models;
use crate::modules::platform_adapter;
use serde_json::json;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

const OAUTH_WAIT_TIMEOUT: Duration = Duration::from_secs(10 * 60 + 30);

#[tauri::command]
pub async fn start_oauth_login(app_handle: AppHandle) -> Result<models::Account, String> {
    let auth_url: String =
        platform_adapter::call_antigravity_series("oauth.prepareUrl", json!({}))?;
    app_handle
        .opener()
        .open_url(&auth_url, None::<String>)
        .map_err(|error| format!("无法打开浏览器: {}", error))?;
    platform_adapter::call_antigravity_series_with_timeout(
        "oauth.complete",
        json!({}),
        OAUTH_WAIT_TIMEOUT,
    )
}

#[tauri::command]
pub async fn complete_oauth_login(_app_handle: AppHandle) -> Result<models::Account, String> {
    platform_adapter::call_antigravity_series_with_timeout(
        "oauth.complete",
        json!({}),
        OAUTH_WAIT_TIMEOUT,
    )
}

#[tauri::command]
pub async fn prepare_oauth_url(_app_handle: AppHandle) -> Result<String, String> {
    platform_adapter::call_antigravity_series("oauth.prepareUrl", json!({}))
}

#[tauri::command]
pub async fn submit_oauth_callback_url(
    _app_handle: AppHandle,
    callback_url: String,
) -> Result<(), String> {
    platform_adapter::call_antigravity_series(
        "oauth.submitCallbackUrl",
        json!({ "callbackUrl": callback_url }),
    )
}

#[tauri::command]
pub async fn cancel_oauth_login() -> Result<(), String> {
    platform_adapter::call_antigravity_series("oauth.cancel", json!({}))
}
