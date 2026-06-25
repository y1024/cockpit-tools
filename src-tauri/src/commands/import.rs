use crate::models;
use crate::modules;
use serde_json::json;
use tauri::AppHandle;

#[tauri::command]
pub async fn import_from_old_tools() -> Result<Vec<models::Account>, String> {
    modules::platform_adapter::call_antigravity_series("accounts.importOldTools", json!({}))
}

#[tauri::command]
pub async fn import_from_local(app: AppHandle) -> Result<models::Account, String> {
    let account =
        modules::platform_adapter::call_antigravity_series("accounts.importLocal", json!({}))?;
    let _ = crate::modules::tray::update_tray_menu(&app);
    Ok(account)
}

#[tauri::command]
pub async fn import_from_json(json_content: String) -> Result<Vec<models::Account>, String> {
    modules::platform_adapter::call_antigravity_series(
        "accounts.importJson",
        json!({ "jsonContent": json_content }),
    )
}

#[tauri::command]
pub async fn import_from_files(
    file_paths: Vec<String>,
) -> Result<modules::import::FileImportResult, String> {
    modules::platform_adapter::call_antigravity_series(
        "accounts.importFiles",
        json!({ "filePaths": file_paths }),
    )
}

#[tauri::command]
pub async fn export_accounts(account_ids: Vec<String>) -> Result<String, String> {
    modules::platform_adapter::call_antigravity_series(
        "accounts.export",
        json!({ "accountIds": account_ids }),
    )
}
