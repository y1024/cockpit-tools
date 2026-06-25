use crate::error::{AppError, AppResult};
use crate::models;
use crate::modules;
use serde::de::DeserializeOwned;
use serde_json::json;
use tauri::AppHandle;
use tauri::Emitter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AntigravityRuntimeTarget {
    Legacy,
    Ide,
}

fn normalize_antigravity_runtime_target(raw: Option<&str>) -> AntigravityRuntimeTarget {
    match raw.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "antigravity" => AntigravityRuntimeTarget::Legacy,
        _ => AntigravityRuntimeTarget::Ide,
    }
}

fn call_antigravity_account_adapter<T: DeserializeOwned>(
    method: &str,
    payload: serde_json::Value,
) -> Result<T, String> {
    modules::platform_adapter::call_antigravity_series(method, payload)
}

fn call_antigravity_account_adapter_for_target<T: DeserializeOwned>(
    target: AntigravityRuntimeTarget,
    method: &str,
    payload: serde_json::Value,
) -> Result<T, String> {
    match target {
        AntigravityRuntimeTarget::Legacy => {
            modules::platform_adapter::call_antigravity(method, payload)
        }
        AntigravityRuntimeTarget::Ide => {
            modules::platform_adapter::call_antigravity_ide(method, payload)
        }
    }
}

fn emit_antigravity_path_missing(
    app: &AppHandle,
    account_id: &str,
    target: AntigravityRuntimeTarget,
) {
    let runtime_target = match target {
        AntigravityRuntimeTarget::Legacy => Some("antigravity"),
        AntigravityRuntimeTarget::Ide => None,
    };
    let mut retry = json!({
        "kind": "switchAccount",
        "accountId": account_id,
    });
    if let Some(runtime_target) = runtime_target {
        if let Some(object) = retry.as_object_mut() {
            object.insert("runtimeTarget".to_string(), json!(runtime_target));
        }
    }
    let _ = app.emit(
        "app:path_missing",
        json!({
            "app": "antigravity",
            "retry": retry,
        }),
    );
}

#[tauri::command]
pub async fn list_accounts() -> Result<Vec<models::Account>, String> {
    call_antigravity_account_adapter("accounts.list", json!({}))
}

/// 从 VS Code SecretStorage 同步插件账号
#[tauri::command]
pub async fn sync_from_extension(app: tauri::AppHandle) -> Result<usize, String> {
    let count = call_antigravity_account_adapter("accounts.syncFromExtension", json!({}))?;
    if count > 0 {
        modules::websocket::broadcast_data_changed("extension_sync");
        let _ = crate::modules::tray::update_tray_menu(&app);
    }
    Ok(count)
}

#[tauri::command]
pub async fn add_account(refresh_token: String) -> Result<models::Account, String> {
    let account = call_antigravity_account_adapter(
        "accounts.addRefreshToken",
        json!({ "refreshToken": refresh_token }),
    )?;
    modules::websocket::broadcast_data_changed("account_added");
    Ok(account)
}

#[tauri::command]
pub async fn delete_account(account_id: String) -> Result<(), String> {
    call_antigravity_account_adapter::<()>("accounts.delete", json!({ "accountId": account_id }))?;
    modules::websocket::broadcast_data_changed("account_deleted");
    Ok(())
}

#[tauri::command]
pub async fn delete_accounts(account_ids: Vec<String>) -> Result<(), String> {
    call_antigravity_account_adapter::<()>(
        "accounts.deleteMany",
        json!({ "accountIds": account_ids }),
    )?;
    modules::websocket::broadcast_data_changed("accounts_deleted");
    Ok(())
}

#[tauri::command]
pub async fn reorder_accounts(account_ids: Vec<String>) -> Result<(), String> {
    call_antigravity_account_adapter("accounts.reorder", json!({ "accountIds": account_ids }))
}

#[tauri::command]
pub async fn get_current_account() -> Result<Option<models::Account>, String> {
    call_antigravity_account_adapter("accounts.current", json!({}))
}

#[tauri::command]
pub async fn set_current_account(app: tauri::AppHandle, account_id: String) -> Result<(), String> {
    call_antigravity_account_adapter::<()>(
        "accounts.setCurrent",
        json!({ "accountId": account_id }),
    )?;
    let _ = crate::modules::tray::update_tray_menu(&app);
    Ok(())
}

#[tauri::command]
pub async fn fetch_account_quota(account_id: String) -> AppResult<models::Account> {
    call_antigravity_account_adapter("accounts.refresh", json!({ "accountId": account_id }))
        .map_err(AppError::Account)
}

#[tauri::command]
pub async fn refresh_all_quotas(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let result = call_antigravity_account_adapter("accounts.refreshAll", json!({}));
    if result.is_ok() {
        let _ = crate::modules::tray::update_tray_menu(&app);
    }
    result
}

#[tauri::command]
pub async fn refresh_current_quota(app: tauri::AppHandle) -> Result<(), String> {
    call_antigravity_account_adapter::<()>("accounts.refreshCurrent", json!({}))?;
    let _ = crate::modules::tray::update_tray_menu(&app);
    Ok(())
}

/// 切换账号（完整流程：Token刷新 + 关闭程序 + 注入 + 重启）
#[tauri::command]
pub async fn switch_account(
    app: AppHandle,
    account_id: String,
    runtime_target: Option<String>,
) -> Result<models::Account, String> {
    let runtime_target = normalize_antigravity_runtime_target(runtime_target.as_deref());
    let result: Result<models::Account, String> = call_antigravity_account_adapter_for_target(
        runtime_target,
        "switch.inject",
        json!({ "accountId": account_id }),
    );
    match result {
        Ok(account) => {
            modules::websocket::broadcast_account_switched(&account.id, &account.email);
            Ok(account)
        }
        Err(error) => {
            if error.starts_with("APP_PATH_NOT_FOUND:") {
                emit_antigravity_path_missing(&app, &account_id, runtime_target);
            }
            Err(error)
        }
    }
}

#[tauri::command]
pub fn load_antigravity_switch_history() -> Result<serde_json::Value, String> {
    call_antigravity_account_adapter("switch.history.load", json!({}))
}

#[tauri::command]
pub fn clear_antigravity_switch_history() -> Result<(), String> {
    call_antigravity_account_adapter("switch.history.clear", json!({}))
}

#[tauri::command]
pub async fn update_account_tags(
    account_id: String,
    tags: Vec<String>,
) -> Result<models::Account, String> {
    let account = call_antigravity_account_adapter(
        "accounts.updateTags",
        json!({ "accountId": account_id, "tags": tags }),
    )?;
    modules::websocket::broadcast_data_changed("account_tags_updated");
    Ok(account)
}

#[tauri::command]
pub async fn update_account_notes(
    account_id: String,
    notes: String,
) -> Result<models::Account, String> {
    call_antigravity_account_adapter(
        "accounts.updateNotes",
        json!({ "accountId": account_id, "notes": notes }),
    )
}

/// 从本地客户端同步当前账号状态
/// 当前实现已禁用“跟随本地客户端当前账号”，保留空结果以兼容旧调用。
#[tauri::command]
pub async fn sync_current_from_client(_app: tauri::AppHandle) -> Result<Option<String>, String> {
    Ok(None)
}

// ─── 账号分组持久化 ────────────────────────────────────────────

#[tauri::command]
pub async fn load_account_groups() -> Result<String, String> {
    call_antigravity_account_adapter("accounts.groups.load", json!({}))
}

#[tauri::command]
pub async fn save_account_groups(data: String) -> Result<(), String> {
    call_antigravity_account_adapter("accounts.groups.save", json!({ "data": data }))
}
