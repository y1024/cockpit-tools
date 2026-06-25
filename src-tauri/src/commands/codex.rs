use crate::models::codex::{
    CodexAccount, CodexApiProviderMode, CodexAppSpeed, CodexAppSpeedConfig, CodexFileImportResult,
    CodexQuickConfig, CodexQuota,
};
use crate::models::codex_local_access::{
    CodexLocalAccessAccountModelRule, CodexLocalAccessChatMessage, CodexLocalAccessChatResult,
    CodexLocalAccessClientBaseUrlHost, CodexLocalAccessCustomRoutingRule,
    CodexLocalAccessGatewayMode, CodexLocalAccessModelAlias, CodexLocalAccessModelPricing,
    CodexLocalAccessPortCleanupResult, CodexLocalAccessRequestKind,
    CodexLocalAccessRoutingStrategy, CodexLocalAccessScope, CodexLocalAccessState,
    CodexLocalAccessTestResult, CodexLocalAccessTimeoutPreset, CodexLocalAccessTimeouts,
    CodexLocalAccessUsageEventPage,
};
use crate::modules::{logger, platform_adapter, process};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tauri::Emitter;
use tauri_plugin_opener::OpenerExt;

static CODEX_POST_REFRESH_CHECK_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
const CODEX_LOCAL_ACCESS_FAST_MUTATION_TIMEOUT: Duration = Duration::from_secs(20);
const CODEX_FAST_READ_TIMEOUT: Duration = Duration::from_secs(15);

fn call_codex_local_access_fast<T: DeserializeOwned>(
    method: &str,
    payload: Value,
) -> Result<T, String> {
    platform_adapter::call_codex_with_timeout(
        method,
        payload,
        CODEX_LOCAL_ACCESS_FAST_MUTATION_TIMEOUT,
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexSwitchPostActions {
    codex_launch_on_switch: bool,
    opencode_restart_app_path: Option<String>,
    restart_specified_app_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SwitchCodexAccountResult {
    account: CodexAccount,
    post_actions: CodexSwitchPostActions,
}

fn restart_codex_specified_app_by_path(path: &str) {
    let path = path.trim();
    if path.is_empty() {
        logger::log_warn("已开启切换 Codex 时自动重启指定应用，但未配置应用路径，已跳过");
        return;
    }

    match process::restart_specified_app_by_path(path, 20) {
        Ok(()) => {
            logger::log_info(&format!("已重启指定应用: {}", path));
        }
        Err(error) => {
            logger::log_warn(&format!("重启指定应用失败（path={}）：{}", path, error));
        }
    }
}

/// 列出所有 Codex 账号
#[tauri::command]
pub fn list_codex_accounts() -> Result<Vec<CodexAccount>, String> {
    platform_adapter::call_codex("accounts.list", json!({}))
}

/// 获取当前激活的 Codex 账号
#[tauri::command]
pub fn get_current_codex_account() -> Result<Option<CodexAccount>, String> {
    platform_adapter::call_codex("accounts.current", json!({}))
}

#[tauri::command]
pub fn get_codex_config_toml_path() -> Result<String, String> {
    platform_adapter::call_codex("config.path", json!({}))
}

#[tauri::command]
pub fn open_codex_config_toml(app: AppHandle) -> Result<(), String> {
    let path = std::path::PathBuf::from(get_codex_config_toml_path()?);
    if !path.exists() {
        return Err(format!("未找到 Codex config.toml 文件: {}", path.display()));
    }

    app.opener()
        .open_path(path.to_string_lossy().to_string(), None::<String>)
        .map_err(|e| format!("打开 Codex config.toml 失败: {}", e))
}

#[tauri::command]
pub fn get_codex_quick_config() -> Result<CodexQuickConfig, String> {
    platform_adapter::call_codex("config.quick.get", json!({}))
}

#[tauri::command]
pub fn save_codex_quick_config(
    model_context_window: Option<i64>,
    auto_compact_token_limit: Option<i64>,
) -> Result<CodexQuickConfig, String> {
    platform_adapter::call_codex(
        "config.quick.save",
        json!({
            "modelContextWindow": model_context_window,
            "autoCompactTokenLimit": auto_compact_token_limit,
        }),
    )
}

#[tauri::command]
pub fn get_codex_app_speed_config() -> Result<CodexAppSpeedConfig, String> {
    platform_adapter::call_codex("config.appSpeed.get", json!({}))
}

#[tauri::command]
pub fn save_codex_app_speed(speed: CodexAppSpeed) -> Result<CodexAppSpeedConfig, String> {
    platform_adapter::call_codex("config.appSpeed.save", json!({ "speed": speed }))
}

#[tauri::command]
pub fn get_codex_api_service_app_speed_config() -> Result<CodexAppSpeedConfig, String> {
    platform_adapter::call_codex("config.apiServiceAppSpeed.get", json!({}))
}

#[tauri::command]
pub fn save_codex_api_service_app_speed(
    speed: CodexAppSpeed,
) -> Result<CodexAppSpeedConfig, String> {
    platform_adapter::call_codex("config.apiServiceAppSpeed.save", json!({ "speed": speed }))
}

#[tauri::command]
pub fn update_codex_account_app_speed(
    account_id: String,
    speed: CodexAppSpeed,
) -> Result<CodexAccount, String> {
    platform_adapter::call_codex(
        "accounts.updateAppSpeed",
        json!({ "accountId": account_id, "speed": speed }),
    )
}

/// 刷新账号资料（团队名/结构）
#[tauri::command]
pub async fn refresh_codex_account_profile(account_id: String) -> Result<CodexAccount, String> {
    platform_adapter::call_codex(
        "accounts.refreshProfile",
        json!({ "accountId": account_id }),
    )
}

/// 切换 Codex 账号（包含 token 刷新检查）
#[tauri::command]
pub async fn switch_codex_account(
    app: AppHandle,
    account_id: String,
    auto_repair_mode: Option<String>,
) -> Result<CodexAccount, String> {
    let flow_started = Instant::now();
    logger::log_info(&format!(
        "[Codex Switch][Backend] switch_codex_account started: account_id={}",
        account_id
    ));

    let switch_started = Instant::now();
    let result: SwitchCodexAccountResult = platform_adapter::call_codex(
        "switch.account",
        json!({
            "accountId": account_id.clone(),
            "autoRepairMode": auto_repair_mode,
        }),
    )?;
    logger::log_info(&format!(
        "[Codex Switch][Backend] switch.account adapter finished: account_id={}, elapsed_ms={}, total_ms={}",
        account_id,
        switch_started.elapsed().as_millis(),
        flow_started.elapsed().as_millis()
    ));

    if let Some(opencode_app_path) = result.post_actions.opencode_restart_app_path.as_deref() {
        let opencode_started = Instant::now();
        if process::is_opencode_running() {
            if let Err(e) = process::close_opencode(20) {
                logger::log_warn(&format!("OpenCode 关闭失败: {}", e));
            }
        } else {
            logger::log_info("OpenCode 未在运行，准备启动");
        }
        if let Err(e) = process::start_opencode_with_path(Some(opencode_app_path)) {
            logger::log_warn(&format!("OpenCode 启动失败: {}", e));
        }
        logger::log_info(&format!(
            "[Codex Switch][Backend] opencode restart post action finished: account_id={}, elapsed_ms={}, total_ms={}",
            account_id,
            opencode_started.elapsed().as_millis(),
            flow_started.elapsed().as_millis()
        ));
    }

    if result.post_actions.codex_launch_on_switch {
        let launch_started = Instant::now();
        #[cfg(target_os = "macos")]
        if process::is_codex_running() {
            logger::log_info("检测到 Codex 正在运行，将按默认实例 PID 逻辑重启");
        }
        match crate::commands::codex_instance::codex_start_default_with_prepared_profile().await {
            Ok(_) => {}
            Err(e) => {
                logger::log_warn(&format!("Codex 启动失败: {}", e));
                if e.starts_with("APP_PATH_NOT_FOUND:") {
                    let _ = app.emit(
                        "app:path_missing",
                        serde_json::json!({ "app": "codex", "retry": { "kind": "default" } }),
                    );
                }
            }
        }
        logger::log_info(&format!(
            "[Codex Switch][Backend] codex_start_default_with_prepared_profile finished: account_id={}, elapsed_ms={}, total_ms={}",
            account_id,
            launch_started.elapsed().as_millis(),
            flow_started.elapsed().as_millis()
        ));
    } else {
        logger::log_info("已关闭切换 Codex 时自动启动 Codex App");
    }

    if let Some(app_path) = result.post_actions.restart_specified_app_path.as_deref() {
        let restart_specified_started = Instant::now();
        restart_codex_specified_app_by_path(app_path);
        logger::log_info(&format!(
            "[Codex Switch][Backend] restart specified app post action finished: account_id={}, elapsed_ms={}, total_ms={}",
            account_id,
            restart_specified_started.elapsed().as_millis(),
            flow_started.elapsed().as_millis()
        ));
    }

    let tray_started = Instant::now();
    let _ = crate::modules::tray::update_tray_menu(&app);
    logger::log_info(&format!(
        "[Codex Switch][Backend] switch_codex_account finished: account_id={}, tray_elapsed_ms={}, total_ms={}",
        account_id,
        tray_started.elapsed().as_millis(),
        flow_started.elapsed().as_millis()
    ));
    Ok(result.account)
}

async fn run_codex_post_refresh_checks(app: &AppHandle) {
    if CODEX_POST_REFRESH_CHECK_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        logger::log_info("[AutoSwitch][Codex] 后置检查进行中，跳过本次执行");
        return;
    }

    let mut switched = false;

    match platform_adapter::call_codex::<Option<CodexAccount>>(
        "accounts.pickAutoSwitchTarget",
        json!({}),
    ) {
        Ok(Some(target)) => {
            let target_id = target.id.clone();
            match switch_codex_account(app.clone(), target_id.clone(), None).await {
                Ok(switched_account) => {
                    logger::log_info(&format!(
                        "[AutoSwitch][Codex] 自动切号完成: target_id={}, email={}",
                        switched_account.id, switched_account.email
                    ));
                    switched = true;
                }
                Err(e) => {
                    logger::log_warn(&format!(
                        "[AutoSwitch][Codex] 自动切号失败: target_id={}, error={}",
                        target_id, e
                    ));
                }
            }
        }
        Ok(None) => {}
        Err(e) => {
            logger::log_warn(&format!("[AutoSwitch][Codex] 自动切号检查失败: {}", e));
        }
    }

    if !switched {
        match platform_adapter::call_codex::<Option<crate::modules::quota_alert::QuotaAlertPayload>>(
            "quota.alertPayload",
            json!({}),
        ) {
            Ok(Some(payload)) => crate::modules::quota_alert::dispatch_quota_alert(&payload),
            Ok(None) => {}
            Err(e) => logger::log_warn(&format!("[QuotaAlert][Codex] 预警检查失败: {}", e)),
        }
    }

    CODEX_POST_REFRESH_CHECK_IN_PROGRESS.store(false, Ordering::SeqCst);
}

/// 删除 Codex 账号
#[tauri::command]
pub async fn delete_codex_account(account_id: String) -> Result<(), String> {
    platform_adapter::call_codex("accounts.delete", json!({ "accountId": account_id }))
}

/// 批量删除 Codex 账号
#[tauri::command]
pub async fn delete_codex_accounts(account_ids: Vec<String>) -> Result<(), String> {
    platform_adapter::call_codex("accounts.deleteMany", json!({ "accountIds": account_ids }))
}

/// 从本地 auth.json 导入账号
#[tauri::command]
pub async fn import_codex_from_local(app: AppHandle) -> Result<CodexAccount, String> {
    let account: CodexAccount =
        platform_adapter::call_codex("accounts.importFromLocal", json!({}))?;
    if !account.is_api_key_auth() {
        run_codex_post_refresh_checks(&app).await;
    }
    let _ = crate::modules::tray::update_tray_menu(&app);
    Ok(account)
}

/// 从 JSON 字符串导入账号
#[tauri::command]
pub async fn import_codex_from_json(
    app: AppHandle,
    json_content: String,
) -> Result<Vec<CodexAccount>, String> {
    let accounts: Vec<CodexAccount> = platform_adapter::call_codex(
        "accounts.importFromJson",
        json!({ "jsonContent": json_content }),
    )?;
    if accounts.iter().any(|account| !account.is_api_key_auth()) {
        run_codex_post_refresh_checks(&app).await;
    }
    if !accounts.is_empty() {
        let _ = crate::modules::tray::update_tray_menu(&app);
    }
    Ok(accounts)
}

/// 导出 Codex 账号
#[tauri::command]
pub fn export_codex_accounts(account_ids: Vec<String>) -> Result<String, String> {
    platform_adapter::call_codex("accounts.export", json!({ "accountIds": account_ids }))
}

/// 从本地文件导入 Codex 账号
#[tauri::command]
pub async fn import_codex_from_files(
    app: AppHandle,
    file_paths: Vec<String>,
) -> Result<CodexFileImportResult, String> {
    let result: CodexFileImportResult = platform_adapter::call_codex(
        "accounts.importFromFiles",
        json!({ "filePaths": file_paths }),
    )?;
    if result
        .imported
        .iter()
        .any(|account| !account.is_api_key_auth())
    {
        run_codex_post_refresh_checks(&app).await;
    }
    if !result.imported.is_empty() {
        let _ = crate::modules::tray::update_tray_menu(&app);
    }
    Ok(result)
}

#[tauri::command]
pub fn start_codex_batch_import_from_files(
    _app: AppHandle,
    file_paths: Vec<String>,
    check_quota: bool,
) -> Result<Value, String> {
    platform_adapter::call_codex(
        "accounts.batchImport.startFromFiles",
        json!({
            "filePaths": file_paths,
            "checkQuota": check_quota,
        }),
    )
}

#[tauri::command]
pub fn cancel_codex_batch_import(session_id: String) -> Result<(), String> {
    platform_adapter::call_codex(
        "accounts.batchImport.cancel",
        json!({ "sessionId": session_id }),
    )
}

#[tauri::command]
pub fn resume_codex_batch_import(_app: AppHandle, session_id: String) -> Result<(), String> {
    platform_adapter::call_codex(
        "accounts.batchImport.resume",
        json!({ "sessionId": session_id }),
    )
}

#[tauri::command]
pub fn get_codex_batch_import_preview(session_id: String) -> Result<Value, String> {
    platform_adapter::call_codex(
        "accounts.batchImport.preview",
        json!({ "sessionId": session_id }),
    )
}

#[tauri::command]
pub fn confirm_codex_batch_import(
    session_id: String,
    item_ids: Vec<String>,
) -> Result<Value, String> {
    platform_adapter::call_codex(
        "accounts.batchImport.confirm",
        json!({
            "sessionId": session_id,
            "itemIds": item_ids,
        }),
    )
}

/// 刷新单个账号配额
#[tauri::command]
pub async fn refresh_codex_quota(app: AppHandle, account_id: String) -> Result<CodexQuota, String> {
    let result = platform_adapter::call_codex("quota.refresh", json!({ "accountId": account_id }));
    if result.is_ok() {
        run_codex_post_refresh_checks(&app).await;
        let _ = crate::modules::tray::update_tray_menu(&app);
    }
    result
}

#[tauri::command]
pub async fn get_codex_reset_credits(account_id: String) -> Result<Value, String> {
    platform_adapter::call_codex("quota.resetCredits", json!({ "accountId": account_id }))
}

#[tauri::command]
pub async fn consume_codex_reset_credit(account_id: String) -> Result<(), String> {
    platform_adapter::call_codex(
        "quota.consumeResetCredit",
        json!({ "accountId": account_id }),
    )
}

#[tauri::command]
pub async fn get_codex_referral_invite_eligibility(
    account_id: String,
    referral_key: Option<String>,
) -> Result<Value, String> {
    platform_adapter::call_codex(
        "quota.referralInviteEligibility",
        json!({ "accountId": account_id, "referralKey": referral_key }),
    )
}

#[tauri::command]
pub async fn get_codex_referral_eligibility_rules(
    account_id: String,
    referral_key: Option<String>,
) -> Result<Value, String> {
    platform_adapter::call_codex(
        "quota.referralEligibilityRules",
        json!({ "accountId": account_id, "referralKey": referral_key }),
    )
}

#[tauri::command]
pub async fn send_codex_referral_invites(
    account_id: String,
    referral_key: Option<String>,
    emails: Vec<String>,
) -> Result<Value, String> {
    platform_adapter::call_codex(
        "quota.sendReferralInvites",
        json!({ "accountId": account_id, "referralKey": referral_key, "emails": emails }),
    )
}

#[tauri::command]
pub async fn refresh_codex_subscription_info(
    app: AppHandle,
    account_id: String,
) -> Result<CodexAccount, String> {
    let result = platform_adapter::call_codex(
        "quota.refreshSubscriptionInfo",
        json!({ "accountId": account_id, "force": true }),
    );
    if result.is_ok() {
        let _ = crate::modules::tray::update_tray_menu(&app);
    }
    result
}

#[tauri::command]
pub async fn refresh_current_codex_quota(app: AppHandle) -> Result<(), String> {
    let result: Result<serde_json::Value, String> =
        platform_adapter::call_codex("quota.refreshCurrent", json!({}));
    if result.is_ok() {
        run_codex_post_refresh_checks(&app).await;
        let _ = crate::modules::tray::update_tray_menu(&app);
        Ok(())
    } else {
        Err(result
            .err()
            .unwrap_or_else(|| "刷新 Codex 配额失败".to_string()))
    }
}

/// 刷新所有账号配额
#[tauri::command]
pub async fn refresh_all_codex_quotas(app: AppHandle) -> Result<i32, String> {
    let results: Vec<(String, Result<CodexQuota, String>)> =
        platform_adapter::call_codex("quota.refreshAll", json!({}))?;
    let success_count = results.iter().filter(|(_, r)| r.is_ok()).count();
    if success_count > 0 {
        run_codex_post_refresh_checks(&app).await;
    }
    let _ = crate::modules::tray::update_tray_menu(&app);
    Ok(success_count as i32)
}

/// OAuth：开始登录（返回 loginId + authUrl）
#[tauri::command]
pub async fn codex_oauth_login_start() -> Result<Value, String> {
    platform_adapter::call_codex("oauth.start", json!({}))
}

/// OAuth：浏览器授权完成后按 loginId 完成登录
#[tauri::command]
pub async fn codex_oauth_login_completed(
    login_id: String,
    reauth_account_id: Option<String>,
) -> Result<CodexAccount, String> {
    platform_adapter::call_codex(
        "oauth.complete",
        json!({
            "loginId": login_id,
            "reauthAccountId": reauth_account_id,
        }),
    )
}

/// OAuth：按 loginId 取消登录（login_id 为空时取消当前流程）
#[tauri::command]
pub fn codex_oauth_login_cancel(login_id: Option<String>) -> Result<(), String> {
    platform_adapter::call_codex("oauth.cancel", json!({ "loginId": login_id }))
}

/// OAuth：手动提交回调链接（用于本地端口不可达时）
#[tauri::command]
pub fn codex_oauth_submit_callback_url(
    login_id: String,
    callback_url: String,
) -> Result<(), String> {
    platform_adapter::call_codex(
        "oauth.submitCallbackUrl",
        json!({ "loginId": login_id, "callbackUrl": callback_url }),
    )
}

/// 通过 Token 添加账号
#[tauri::command]
pub async fn add_codex_account_with_token(
    id_token: String,
    access_token: String,
    refresh_token: Option<String>,
) -> Result<CodexAccount, String> {
    platform_adapter::call_codex(
        "accounts.addToken",
        json!({
            "idToken": id_token,
            "accessToken": access_token,
            "refreshToken": refresh_token,
        }),
    )
}

/// 通过 API Key 添加账号
#[tauri::command]
pub fn add_codex_account_with_api_key(
    api_key: String,
    api_base_url: Option<String>,
    api_provider_mode: Option<CodexApiProviderMode>,
    api_provider_id: Option<String>,
    api_provider_name: Option<String>,
    api_model_catalog: Option<Vec<String>>,
    api_wire_api: Option<String>,
    api_supports_vision: Option<bool>,
    api_model_vision_support: Option<std::collections::HashMap<String, bool>>,
    api_vision_routing_model: Option<String>,
    account_name: Option<String>,
) -> Result<CodexAccount, String> {
    platform_adapter::call_codex(
        "accounts.addApiKey",
        json!({
            "apiKey": api_key,
            "apiBaseUrl": api_base_url,
            "apiProviderMode": api_provider_mode,
            "apiProviderId": api_provider_id,
            "apiProviderName": api_provider_name,
            "apiModelCatalog": api_model_catalog,
            "apiWireApi": api_wire_api,
            "apiSupportsVision": api_supports_vision,
            "apiModelVisionSupport": api_model_vision_support,
            "apiVisionRoutingModel": api_vision_routing_model,
            "accountName": account_name,
        }),
    )
}

#[tauri::command]
pub fn update_codex_account_name(account_id: String, name: String) -> Result<CodexAccount, String> {
    platform_adapter::call_codex(
        "accounts.updateName",
        json!({ "accountId": account_id, "name": name }),
    )
}

#[tauri::command]
pub fn update_codex_api_key_credentials(
    account_id: String,
    api_key: String,
    api_base_url: Option<String>,
    api_provider_mode: Option<CodexApiProviderMode>,
    api_provider_id: Option<String>,
    api_provider_name: Option<String>,
    api_model_catalog: Option<Vec<String>>,
    api_wire_api: Option<String>,
    api_supports_vision: Option<bool>,
    api_model_vision_support: Option<std::collections::HashMap<String, bool>>,
    api_vision_routing_model: Option<String>,
) -> Result<CodexAccount, String> {
    platform_adapter::call_codex(
        "accounts.updateApiKeyCredentials",
        json!({
            "accountId": account_id,
            "apiKey": api_key,
            "apiBaseUrl": api_base_url,
            "apiProviderMode": api_provider_mode,
            "apiProviderId": api_provider_id,
            "apiProviderName": api_provider_name,
            "apiModelCatalog": api_model_catalog,
            "apiWireApi": api_wire_api,
            "apiSupportsVision": api_supports_vision,
            "apiModelVisionSupport": api_model_vision_support,
            "apiVisionRoutingModel": api_vision_routing_model,
        }),
    )
}

#[tauri::command]
pub async fn update_codex_api_key_bound_oauth_account(
    account_id: String,
    bound_oauth_account_id: Option<String>,
    bound_oauth_use_local_gateway: Option<bool>,
) -> Result<CodexAccount, String> {
    platform_adapter::call_codex(
        "accounts.updateApiKeyBoundOAuthAccount",
        json!({
            "accountId": account_id,
            "boundOauthAccountId": bound_oauth_account_id,
            "boundOauthUseLocalGateway": bound_oauth_use_local_gateway,
        }),
    )
}

#[tauri::command]
pub async fn update_codex_account_tags(
    account_id: String,
    tags: Vec<String>,
) -> Result<CodexAccount, String> {
    platform_adapter::call_codex(
        "accounts.updateTags",
        json!({ "accountId": account_id, "tags": tags }),
    )
}

#[tauri::command]
pub async fn update_codex_account_note(
    account_id: String,
    note: String,
) -> Result<CodexAccount, String> {
    platform_adapter::call_codex(
        "accounts.updateNote",
        json!({ "accountId": account_id, "note": note }),
    )
}

/// 检查 Codex OAuth 端口是否被占用
#[tauri::command]
pub fn is_codex_oauth_port_in_use() -> Result<bool, String> {
    platform_adapter::call_codex("oauth.isPortInUse", json!({}))
}

/// 关闭占用 Codex OAuth 端口的进程
#[tauri::command]
pub fn close_codex_oauth_port() -> Result<u32, String> {
    platform_adapter::call_codex("oauth.closePortProcess", json!({}))
}

#[tauri::command]
pub fn codex_wakeup_get_cli_status() -> Result<Value, String> {
    platform_adapter::call_codex("wakeup.getCliStatus", json!({}))
}

#[tauri::command]
pub fn codex_wakeup_update_runtime_config(
    codex_cli_path: Option<String>,
    node_path: Option<String>,
) -> Result<Value, String> {
    platform_adapter::call_codex(
        "wakeup.updateRuntimeConfig",
        json!({
            "codexCliPath": codex_cli_path,
            "nodePath": node_path,
        }),
    )
}

#[tauri::command]
pub fn codex_wakeup_get_overview() -> Result<Value, String> {
    platform_adapter::call_codex_with_timeout(
        "wakeup.getOverview",
        json!({}),
        CODEX_FAST_READ_TIMEOUT,
    )
}

#[tauri::command]
pub fn codex_wakeup_get_state() -> Result<Value, String> {
    platform_adapter::call_codex_with_timeout("wakeup.getState", json!({}), CODEX_FAST_READ_TIMEOUT)
}

#[tauri::command]
pub fn codex_wakeup_save_state(
    enabled: bool,
    tasks: Vec<Value>,
    model_presets: Vec<Value>,
    model_preset_migrations: Vec<String>,
) -> Result<Value, String> {
    platform_adapter::call_codex(
        "wakeup.saveState",
        json!({
            "enabled": enabled,
            "tasks": tasks,
            "modelPresets": model_presets,
            "modelPresetMigrations": model_preset_migrations,
        }),
    )
}

#[tauri::command]
pub fn codex_wakeup_load_history() -> Result<Value, String> {
    platform_adapter::call_codex("wakeup.loadHistory", json!({}))
}

#[tauri::command]
pub fn codex_wakeup_clear_history() -> Result<(), String> {
    platform_adapter::call_codex("wakeup.clearHistory", json!({}))
}

#[tauri::command]
pub fn codex_wakeup_cancel_scope(cancel_scope_id: String) -> Result<(), String> {
    platform_adapter::call_codex(
        "wakeup.cancelScope",
        json!({ "cancelScopeId": cancel_scope_id }),
    )
}

#[tauri::command]
pub fn codex_wakeup_release_scope(cancel_scope_id: String) -> Result<(), String> {
    platform_adapter::call_codex(
        "wakeup.releaseScope",
        json!({ "cancelScopeId": cancel_scope_id }),
    )
}

#[tauri::command]
pub async fn codex_wakeup_test(
    _app: AppHandle,
    account_ids: Vec<String>,
    prompt: Option<String>,
    model: Option<String>,
    model_display_name: Option<String>,
    model_reasoning_effort: Option<String>,
    run_id: Option<String>,
    cancel_scope_id: Option<String>,
) -> Result<Value, String> {
    platform_adapter::call_codex(
        "wakeup.test",
        json!({
            "accountIds": account_ids,
            "prompt": prompt,
            "model": model,
            "modelDisplayName": model_display_name,
            "modelReasoningEffort": model_reasoning_effort,
            "runId": run_id,
            "cancelScopeId": cancel_scope_id,
        }),
    )
}

#[tauri::command]
pub async fn codex_wakeup_run_task(
    _app: AppHandle,
    task_id: String,
    run_id: Option<String>,
) -> Result<Value, String> {
    platform_adapter::call_codex(
        "wakeup.runTask",
        json!({
            "taskId": task_id,
            "runId": run_id,
        }),
    )
}

#[tauri::command]
pub async fn codex_wakeup_run_enabled_tasks(
    _app: AppHandle,
    trigger_type: Option<String>,
) -> Result<u32, String> {
    platform_adapter::call_codex(
        "wakeup.runEnabledTasks",
        json!({ "triggerType": trigger_type }),
    )
}

// ─── Codex 账号分组持久化 ────────────────────────────────────────────

#[tauri::command]
pub async fn load_codex_account_groups() -> Result<String, String> {
    platform_adapter::call_codex("accounts.loadGroups", json!({}))
}

#[tauri::command]
pub async fn save_codex_account_groups(data: String) -> Result<(), String> {
    platform_adapter::call_codex("accounts.saveGroups", json!({ "data": data }))
}

#[tauri::command]
pub async fn load_codex_model_providers() -> Result<String, String> {
    platform_adapter::call_codex_with_timeout(
        "modelProviders.load",
        json!({}),
        CODEX_FAST_READ_TIMEOUT,
    )
}

#[tauri::command]
pub async fn save_codex_model_providers(data: String) -> Result<(), String> {
    platform_adapter::call_codex_with_timeout(
        "modelProviders.save",
        json!({ "data": data }),
        CODEX_LOCAL_ACCESS_FAST_MUTATION_TIMEOUT,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelProviderChatTestTarget {
    pub provider_id: String,
    pub provider_name: String,
    pub base_url: String,
    pub api_key_id: Option<String>,
    pub api_key_name: Option<String>,
    pub api_key: String,
    pub wire_api: Option<String>,
    #[serde(default)]
    pub model_catalog: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelProviderChatTestRecord {
    pub provider_id: String,
    pub provider_name: String,
    pub api_key_id: Option<String>,
    pub api_key_name: Option<String>,
    pub wire_api: String,
    pub access_mode: String,
    pub model_id: Option<String>,
    pub success: bool,
    pub prompt: String,
    pub reply: Option<String>,
    pub error: Option<String>,
    pub duration_ms: Option<u64>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelProviderChatTestBatchResult {
    pub run_id: String,
    pub records: Vec<CodexModelProviderChatTestRecord>,
    pub success_count: usize,
    pub failure_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelProviderUsageDetail {
    pub key: String,
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelProviderModel {
    pub id: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelProviderModelsResult {
    pub models: Vec<CodexModelProviderModel>,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelProviderUsageSummary {
    pub mode: Option<String>,
    pub is_valid: Option<bool>,
    pub status: Option<String>,
    pub plan_name: Option<String>,
    pub remaining: Option<f64>,
    pub balance: Option<f64>,
    pub unit: Option<String>,
    pub quota_unlimited: Option<bool>,
    pub quota_limit: Option<f64>,
    pub quota_used: Option<f64>,
    pub quota_remaining: Option<f64>,
    pub today_requests: Option<i64>,
    pub today_total_tokens: Option<i64>,
    pub today_cost: Option<f64>,
    pub total_requests: Option<i64>,
    pub total_total_tokens: Option<i64>,
    pub total_cost: Option<f64>,
    pub model_stats_count: usize,
    pub latency_ms: u64,
    pub details: Vec<CodexModelProviderUsageDetail>,
}

#[tauri::command]
pub async fn codex_test_model_provider_connection(
    base_url: String,
    api_key: String,
    wire_api: Option<String>,
) -> Result<CodexLocalAccessTestResult, String> {
    platform_adapter::call_codex(
        "modelProviders.testConnection",
        json!({
            "baseUrl": base_url,
            "apiKey": api_key,
            "wireApi": wire_api,
        }),
    )
}

#[tauri::command]
pub async fn codex_model_provider_chat_test_batch(
    targets: Vec<CodexModelProviderChatTestTarget>,
    prompt: Option<String>,
    model: Option<String>,
    run_id: Option<String>,
) -> Result<CodexModelProviderChatTestBatchResult, String> {
    platform_adapter::call_codex(
        "modelProviders.chatTestBatch",
        json!({
            "targets": targets,
            "prompt": prompt,
            "model": model,
            "runId": run_id,
        }),
    )
}

#[tauri::command]
pub async fn codex_list_model_provider_models(
    base_url: String,
    api_key: String,
) -> Result<CodexModelProviderModelsResult, String> {
    platform_adapter::call_codex(
        "modelProviders.listModels",
        json!({
            "baseUrl": base_url,
            "apiKey": api_key,
        }),
    )
}

#[tauri::command]
pub async fn codex_query_model_provider_usage(
    base_url: String,
    api_key: String,
    integration_type: Option<String>,
) -> Result<CodexModelProviderUsageSummary, String> {
    platform_adapter::call_codex(
        "modelProviders.queryUsage",
        json!({
            "baseUrl": base_url,
            "apiKey": api_key,
            "integrationType": integration_type,
        }),
    )
}

#[tauri::command]
pub async fn codex_local_access_get_state() -> Result<CodexLocalAccessState, String> {
    platform_adapter::call_codex("localAccess.getState", json!({}))
}

#[tauri::command]
pub async fn codex_local_access_save_accounts(
    account_ids: Vec<String>,
    restrict_free_accounts: Option<bool>,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.saveAccounts",
        json!({
            "accountIds": account_ids,
            "restrictFreeAccounts": restrict_free_accounts,
        }),
    )
}

#[tauri::command]
pub async fn codex_local_access_remove_account(
    account_id: String,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.removeAccount",
        json!({ "accountId": account_id }),
    )
}

#[tauri::command]
pub async fn codex_local_access_rotate_api_key() -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast("localAccess.rotateApiKey", json!({}))
}

#[tauri::command]
pub async fn codex_local_access_update_bound_oauth_account(
    bound_oauth_account_id: Option<String>,
    bound_oauth_use_local_gateway: Option<bool>,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.updateBoundOAuthAccount",
        json!({
            "boundOauthAccountId": bound_oauth_account_id,
            "boundOauthUseLocalGateway": bound_oauth_use_local_gateway,
        }),
    )
}

#[tauri::command]
pub async fn codex_local_access_clear_stats() -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast("localAccess.clearStats", json!({}))
}

#[tauri::command]
pub async fn codex_local_access_query_request_logs(
    page: u32,
    page_size: u32,
    stats_range: Option<String>,
    model_query: Option<String>,
    account_query: Option<String>,
    api_key_query: Option<String>,
    gateway_mode: Option<CodexLocalAccessGatewayMode>,
    request_kind: Option<CodexLocalAccessRequestKind>,
    success: Option<bool>,
    error_category: Option<String>,
) -> Result<CodexLocalAccessUsageEventPage, String> {
    platform_adapter::call_codex(
        "localAccess.queryRequestLogs",
        json!({
            "page": page,
            "pageSize": page_size,
            "statsRange": stats_range,
            "modelQuery": model_query,
            "accountQuery": account_query,
            "apiKeyQuery": api_key_query,
            "gatewayMode": gateway_mode,
            "requestKind": request_kind,
            "success": success,
            "errorCategory": error_category,
        }),
    )
}

#[tauri::command]
pub async fn codex_local_access_prepare_restart() -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast("localAccess.prepareRestart", json!({}))
}

#[tauri::command]
pub async fn codex_local_access_kill_port() -> Result<CodexLocalAccessPortCleanupResult, String> {
    call_codex_local_access_fast("localAccess.killPort", json!({}))
}

#[tauri::command]
pub async fn codex_local_access_update_port(port: u16) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast("localAccess.updatePort", json!({ "port": port }))
}

#[tauri::command]
pub async fn codex_local_access_update_routing_strategy(
    strategy: CodexLocalAccessRoutingStrategy,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.updateRoutingStrategy",
        json!({ "strategy": strategy }),
    )
}

#[tauri::command]
pub async fn codex_local_access_update_custom_routing(
    rules: Vec<CodexLocalAccessCustomRoutingRule>,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast("localAccess.updateCustomRouting", json!({ "rules": rules }))
}

#[tauri::command]
pub async fn codex_local_access_update_account_model_rules(
    rules: Vec<CodexLocalAccessAccountModelRule>,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.updateAccountModelRules",
        json!({ "rules": rules }),
    )
}

#[tauri::command]
pub async fn codex_local_access_update_model_rules(
    model_aliases: Vec<CodexLocalAccessModelAlias>,
    excluded_models: Vec<String>,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.updateModelRules",
        json!({
            "modelAliases": model_aliases,
            "excludedModels": excluded_models,
        }),
    )
}

#[tauri::command]
pub async fn codex_local_access_update_model_pricings(
    model_pricings: Vec<CodexLocalAccessModelPricing>,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.updateModelPricings",
        json!({ "modelPricings": model_pricings }),
    )
}

#[tauri::command]
pub async fn codex_local_access_update_routing_options(
    session_affinity: bool,
    session_affinity_ttl_ms: i64,
    max_retry_credentials: u16,
    max_retry_interval_ms: u64,
    disable_cooling: bool,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.updateRoutingOptions",
        json!({
            "sessionAffinity": session_affinity,
            "sessionAffinityTtlMs": session_affinity_ttl_ms,
            "maxRetryCredentials": max_retry_credentials,
            "maxRetryIntervalMs": max_retry_interval_ms,
            "disableCooling": disable_cooling,
        }),
    )
}

#[tauri::command]
pub async fn codex_local_access_update_timeouts(
    timeouts: CodexLocalAccessTimeouts,
    active_timeout_preset_id: Option<String>,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.updateTimeouts",
        json!({
            "timeouts": timeouts,
            "activeTimeoutPresetId": active_timeout_preset_id,
        }),
    )
}

#[tauri::command]
pub async fn codex_local_access_update_timeout_presets(
    timeout_presets: Vec<CodexLocalAccessTimeoutPreset>,
    active_timeout_preset_id: Option<String>,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.updateTimeoutPresets",
        json!({
            "timeoutPresets": timeout_presets,
            "activeTimeoutPresetId": active_timeout_preset_id,
        }),
    )
}

#[tauri::command]
pub async fn codex_local_access_update_upstream_proxy_config(
    upstream_proxy_url: Option<String>,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.updateUpstreamProxyConfig",
        json!({ "upstreamProxyUrl": upstream_proxy_url }),
    )
}

#[tauri::command]
pub async fn codex_local_access_update_gateway_mode(
    gateway_mode: CodexLocalAccessGatewayMode,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.updateGatewayMode",
        json!({ "gatewayMode": gateway_mode }),
    )
}

#[tauri::command]
pub async fn codex_local_access_update_debug_logs(
    debug_logs: bool,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.updateDebugLogs",
        json!({ "debugLogs": debug_logs }),
    )
}

#[tauri::command]
pub async fn codex_local_access_update_access_scope(
    access_scope: CodexLocalAccessScope,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.updateAccessScope",
        json!({ "accessScope": access_scope }),
    )
}

#[tauri::command]
pub async fn codex_local_access_update_client_base_url_host(
    client_base_url_host: CodexLocalAccessClientBaseUrlHost,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.updateClientBaseUrlHost",
        json!({ "clientBaseUrlHost": client_base_url_host }),
    )
}

#[tauri::command]
pub async fn codex_local_access_update_image_generation_mode(
    image_generation_mode: crate::models::codex_local_access::CodexLocalAccessImageGenerationMode,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.updateImageGenerationMode",
        json!({ "imageGenerationMode": image_generation_mode }),
    )
}

#[tauri::command]
pub async fn codex_local_access_create_api_key(
    label: Option<String>,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast("localAccess.createApiKey", json!({ "label": label }))
}

#[tauri::command]
pub async fn codex_local_access_update_api_key(
    api_key_id: String,
    label: Option<String>,
    enabled: Option<bool>,
    model_prefix: Option<String>,
    allowed_models: Option<Vec<String>>,
    excluded_models: Option<Vec<String>>,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.updateApiKey",
        json!({
            "apiKeyId": api_key_id,
            "label": label,
            "enabled": enabled,
            "modelPrefix": model_prefix,
            "allowedModels": allowed_models,
            "excludedModels": excluded_models,
        }),
    )
}

#[tauri::command]
pub async fn codex_local_access_rotate_named_api_key(
    api_key_id: String,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.rotateNamedApiKey",
        json!({ "apiKeyId": api_key_id }),
    )
}

#[tauri::command]
pub async fn codex_local_access_delete_api_key(
    api_key_id: String,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast(
        "localAccess.deleteApiKey",
        json!({ "apiKeyId": api_key_id }),
    )
}

#[tauri::command]
pub async fn codex_local_access_set_enabled(
    enabled: bool,
) -> Result<CodexLocalAccessState, String> {
    call_codex_local_access_fast("localAccess.setEnabled", json!({ "enabled": enabled }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexLocalAccessActivateResult {
    state: CodexLocalAccessState,
    launch_on_switch: bool,
}

#[tauri::command]
pub async fn codex_local_access_activate(
    app: AppHandle,
    auto_repair_mode: Option<String>,
) -> Result<CodexLocalAccessState, String> {
    let flow_started = Instant::now();
    logger::log_info("[Codex API Service Switch][Backend] codex_local_access_activate started");
    let activate_started = Instant::now();
    let result: CodexLocalAccessActivateResult = platform_adapter::call_codex(
        "localAccess.activate",
        json!({ "autoRepairMode": auto_repair_mode }),
    )?;
    logger::log_info(&format!(
        "[Codex API Service Switch][Backend] localAccess.activate adapter finished: elapsed_ms={}, total_ms={}",
        activate_started.elapsed().as_millis(),
        flow_started.elapsed().as_millis()
    ));

    if result.launch_on_switch {
        let launch_started = Instant::now();
        #[cfg(target_os = "macos")]
        if process::is_codex_running() {
            logger::log_info("检测到 Codex 正在运行，将按默认实例 PID 逻辑重启");
        }
        match crate::commands::codex_instance::codex_start_default_with_prepared_profile().await {
            Ok(_) => {}
            Err(e) => {
                logger::log_warn(&format!("Codex 启动失败: {}", e));
                if e.starts_with("APP_PATH_NOT_FOUND:") {
                    let _ = app.emit(
                        "app:path_missing",
                        serde_json::json!({ "app": "codex", "retry": { "kind": "default" } }),
                    );
                }
            }
        }
        logger::log_info(&format!(
            "[Codex API Service Switch][Backend] codex_start_default_with_prepared_profile finished: elapsed_ms={}, total_ms={}",
            launch_started.elapsed().as_millis(),
            flow_started.elapsed().as_millis()
        ));
    } else {
        logger::log_info("已关闭切换 Codex 时自动启动 Codex App");
    }

    let tray_started = Instant::now();
    let _ = crate::modules::tray::update_tray_menu(&app);
    logger::log_info(&format!(
        "[Codex API Service Switch][Backend] codex_local_access_activate finished: tray_elapsed_ms={}, total_ms={}",
        tray_started.elapsed().as_millis(),
        flow_started.elapsed().as_millis()
    ));
    Ok(result.state)
}

#[tauri::command]
pub async fn codex_local_access_test() -> Result<CodexLocalAccessTestResult, String> {
    platform_adapter::call_codex("localAccess.test", json!({}))
}

#[tauri::command]
pub async fn codex_local_access_chat_test(
    model_id: String,
    messages: Vec<CodexLocalAccessChatMessage>,
) -> Result<CodexLocalAccessChatResult, String> {
    platform_adapter::call_codex(
        "localAccess.chatTest",
        json!({
            "modelId": model_id,
            "messages": messages,
        }),
    )
}

#[tauri::command]
pub async fn codex_local_access_chat_test_stream(
    session_id: String,
    model_id: String,
    messages: Vec<CodexLocalAccessChatMessage>,
) -> Result<(), String> {
    platform_adapter::call_codex(
        "localAccess.chatTestStream",
        json!({
            "sessionId": session_id,
            "modelId": model_id,
            "messages": messages,
        }),
    )
}
