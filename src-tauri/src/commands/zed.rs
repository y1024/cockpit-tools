use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::models::zed::{ZedAccount, ZedOAuthStartResponse, ZedRuntimeStatus};
use crate::modules::{logger, platform_adapter, platform_package, quota_alert};

const ZED_FAST_LOCAL_MUTATION_TIMEOUT: Duration = Duration::from_secs(20);

fn ensure_zed_package_installed() -> Result<(), String> {
    platform_package::ensure_platform_package_installed("zed")
}

fn zed_call<T: DeserializeOwned>(method: &str, payload: Value) -> Result<T, String> {
    ensure_zed_package_installed()?;
    platform_adapter::call_zed(method, payload)
}

async fn zed_call_async<T>(method: &'static str, payload: Value) -> Result<T, String>
where
    T: DeserializeOwned + Send + 'static,
{
    ensure_zed_package_installed()?;
    tauri::async_runtime::spawn_blocking(move || platform_adapter::call_zed(method, payload))
        .await
        .map_err(|error| format!("Zed adapter 任务失败: {}", error))?
}

async fn zed_call_async_with_timeout<T>(
    method: &'static str,
    payload: Value,
    timeout: Duration,
) -> Result<T, String>
where
    T: DeserializeOwned + Send + 'static,
{
    ensure_zed_package_installed()?;
    tauri::async_runtime::spawn_blocking(move || {
        platform_adapter::call_zed_with_timeout(method, payload, timeout)
    })
    .await
    .map_err(|error| format!("Zed adapter 任务失败: {}", error))?
}

fn update_tray_menu_in_background(app: AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let _ = crate::modules::tray::update_tray_menu(&app);
    });
}

fn dispatch_zed_quota_alert_if_needed() -> Result<(), String> {
    let payload: Option<quota_alert::QuotaAlertPayload> =
        zed_call("quota.alertPayload", json!({}))?;
    if let Some(payload) = payload.as_ref() {
        quota_alert::dispatch_quota_alert(payload);
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SwitchResult {
    message: String,
    #[serde(default)]
    restart_error: Option<String>,
    path_missing: bool,
}

fn emit_zed_path_missing(app: &AppHandle, retry: Value) {
    let _ = app.emit(
        "app:path_missing",
        json!({
            "app": "zed",
            "retry": retry
        }),
    );
}

#[tauri::command]
pub async fn list_zed_accounts() -> Result<Vec<ZedAccount>, String> {
    zed_call_async_with_timeout("accounts.list", json!({}), ZED_FAST_LOCAL_MUTATION_TIMEOUT).await
}

#[tauri::command]
pub async fn delete_zed_account(app: AppHandle, account_id: String) -> Result<(), String> {
    zed_call_async_with_timeout::<()>(
        "accounts.delete",
        json!({ "accountId": account_id }),
        ZED_FAST_LOCAL_MUTATION_TIMEOUT,
    )
    .await?;
    update_tray_menu_in_background(app);
    Ok(())
}

#[tauri::command]
pub async fn delete_zed_accounts(app: AppHandle, account_ids: Vec<String>) -> Result<(), String> {
    zed_call_async_with_timeout::<()>(
        "accounts.deleteMany",
        json!({ "accountIds": account_ids }),
        ZED_FAST_LOCAL_MUTATION_TIMEOUT,
    )
    .await?;
    update_tray_menu_in_background(app);
    Ok(())
}

#[tauri::command]
pub fn import_zed_from_json(
    app: AppHandle,
    json_content: String,
) -> Result<Vec<ZedAccount>, String> {
    let accounts = zed_call(
        "accounts.importJson",
        json!({ "jsonContent": json_content }),
    )?;
    let _ = crate::modules::tray::update_tray_menu(&app);
    Ok(accounts)
}

#[tauri::command]
pub async fn import_zed_from_local(app: AppHandle) -> Result<Vec<ZedAccount>, String> {
    let account: ZedAccount = zed_call_async("accounts.importLocal", json!({})).await?;
    let _ = crate::modules::tray::update_tray_menu(&app);
    Ok(vec![account])
}

#[tauri::command]
pub fn export_zed_accounts(account_ids: Vec<String>) -> Result<String, String> {
    zed_call("accounts.export", json!({ "accountIds": account_ids }))
}

#[tauri::command]
pub async fn refresh_zed_token(app: AppHandle, account_id: String) -> Result<ZedAccount, String> {
    let started_at = Instant::now();
    logger::log_info(&format!(
        "[Zed Command] 手动刷新账号开始: account_id={}",
        account_id
    ));
    let account: ZedAccount =
        zed_call_async("accounts.refresh", json!({ "accountId": account_id })).await?;
    if let Err(err) = dispatch_zed_quota_alert_if_needed() {
        logger::log_warn(&format!("[QuotaAlert][Zed] 刷新后预警检查失败: {}", err));
    }
    let _ = crate::modules::tray::update_tray_menu(&app);
    logger::log_info(&format!(
        "[Zed Command] 手动刷新账号完成: account_id={}, elapsed={}ms",
        account.id,
        started_at.elapsed().as_millis()
    ));
    Ok(account)
}

#[tauri::command]
pub async fn refresh_all_zed_tokens(app: AppHandle) -> Result<i32, String> {
    let started_at = Instant::now();
    logger::log_info("[Zed Command] 批量刷新开始");
    let refreshed: Vec<ZedAccount> = zed_call_async("accounts.refreshAll", json!({})).await?;
    if !refreshed.is_empty() {
        if let Err(err) = dispatch_zed_quota_alert_if_needed() {
            logger::log_warn(&format!(
                "[QuotaAlert][Zed] 全量刷新后预警检查失败: {}",
                err
            ));
        }
    }
    let _ = crate::modules::tray::update_tray_menu(&app);
    logger::log_info(&format!(
        "[Zed Command] 批量刷新完成: refreshed={}, elapsed={}ms",
        refreshed.len(),
        started_at.elapsed().as_millis()
    ));
    Ok(refreshed.len() as i32)
}

#[tauri::command]
pub fn update_zed_account_tags(
    account_id: String,
    tags: Vec<String>,
) -> Result<ZedAccount, String> {
    zed_call(
        "accounts.updateTags",
        json!({ "accountId": account_id, "tags": tags }),
    )
}

#[tauri::command]
pub async fn zed_oauth_login_start() -> Result<ZedOAuthStartResponse, String> {
    let started_at = Instant::now();
    logger::log_info("[Zed OAuth] start 命令触发");
    let result: Result<ZedOAuthStartResponse, String> =
        zed_call_async("oauth.start", json!({})).await;
    match &result {
        Ok(response) => logger::log_info(&format!(
            "[Zed OAuth] start 命令完成: login_id={}, elapsed={}ms",
            response.login_id,
            started_at.elapsed().as_millis()
        )),
        Err(err) => logger::log_warn(&format!(
            "[Zed OAuth] start 命令失败: elapsed={}ms, error={}",
            started_at.elapsed().as_millis(),
            err
        )),
    }
    result
}

#[tauri::command]
pub fn zed_oauth_login_peek() -> Option<ZedOAuthStartResponse> {
    if ensure_zed_package_installed().is_err() {
        return None;
    }
    zed_call("oauth.peek", json!({})).ok()
}

#[tauri::command]
pub async fn zed_oauth_login_complete(
    app: AppHandle,
    login_id: String,
) -> Result<ZedAccount, String> {
    let started_at = Instant::now();
    logger::log_info(&format!(
        "[Zed OAuth] complete 命令触发: login_id={}",
        login_id
    ));
    let account: ZedAccount =
        zed_call_async("oauth.complete", json!({ "loginId": login_id })).await?;
    let _ = crate::modules::tray::update_tray_menu(&app);
    logger::log_info(&format!(
        "[Zed OAuth] complete 命令完成: account_id={}, elapsed={}ms",
        account.id,
        started_at.elapsed().as_millis()
    ));
    Ok(account)
}

#[tauri::command]
pub fn zed_oauth_login_cancel(login_id: Option<String>) -> Result<(), String> {
    zed_call("oauth.cancel", json!({ "loginId": login_id }))
}

#[tauri::command]
pub fn zed_oauth_submit_callback_url(login_id: String, callback_url: String) -> Result<(), String> {
    zed_call(
        "oauth.submitCallbackUrl",
        json!({ "loginId": login_id, "callbackUrl": callback_url }),
    )
}

#[tauri::command]
pub async fn inject_zed_account(app: AppHandle, account_id: String) -> Result<String, String> {
    let started_at = Instant::now();
    logger::log_info(&format!(
        "[Zed Switch] 开始切换账号: account_id={}",
        account_id
    ));

    let result: SwitchResult =
        zed_call_async("switch.inject", json!({ "accountId": account_id })).await?;
    let _ = crate::modules::tray::update_tray_menu(&app);

    if result.path_missing {
        emit_zed_path_missing(
            &app,
            json!({ "kind": "switchAccount", "accountId": account_id }),
        );
        if let Some(error) = result.restart_error.as_deref() {
            logger::log_warn(&format!("[Zed Switch] 切号完成但重启失败: err={}", error));
        }
        return Ok(result.message);
    }

    logger::log_info(&format!(
        "[Zed Switch] 切号成功: elapsed={}ms",
        started_at.elapsed().as_millis()
    ));
    Ok(result.message)
}

#[tauri::command]
pub async fn zed_logout_current_account(app: AppHandle) -> Result<String, String> {
    let result: SwitchResult = zed_call_async("switch.logoutCurrent", json!({})).await?;
    let _ = crate::modules::tray::update_tray_menu(&app);

    if result.path_missing {
        emit_zed_path_missing(&app, json!({ "kind": "default" }));
        if let Some(error) = result.restart_error.as_deref() {
            logger::log_warn(&format!(
                "[Zed Switch] 退出当前账号完成但重启失败: err={}",
                error
            ));
        }
    }

    Ok(result.message)
}

#[tauri::command]
pub fn zed_get_runtime_status() -> Result<ZedRuntimeStatus, String> {
    zed_call("runtime.status", json!({}))
}

#[tauri::command]
pub fn zed_start_default_session() -> Result<ZedRuntimeStatus, String> {
    zed_call("runtime.startDefault", json!({}))
}

#[tauri::command]
pub fn zed_stop_default_session() -> Result<ZedRuntimeStatus, String> {
    zed_call("runtime.stopDefault", json!({}))
}

#[tauri::command]
pub fn zed_restart_default_session() -> Result<ZedRuntimeStatus, String> {
    zed_call("runtime.restartDefault", json!({}))
}

#[tauri::command]
pub fn zed_focus_default_session() -> Result<ZedRuntimeStatus, String> {
    zed_call("runtime.focusDefault", json!({}))
}
