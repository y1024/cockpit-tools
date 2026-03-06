use crate::models::codex::{CodexAccount, CodexQuota, CodexTokens};
use crate::modules::{
    codex_account, codex_oauth, codex_quota, config, logger, opencode_auth, process,
};
use tauri::AppHandle;
use tauri::Emitter;

/// 列出所有 Codex 账号
#[tauri::command]
pub fn list_codex_accounts() -> Result<Vec<CodexAccount>, String> {
    Ok(codex_account::list_accounts())
}

/// 获取当前激活的 Codex 账号
#[tauri::command]
pub fn get_current_codex_account() -> Result<Option<CodexAccount>, String> {
    Ok(codex_account::get_current_account())
}

/// 切换 Codex 账号（包含 token 刷新检查）
#[tauri::command]
pub async fn switch_codex_account(
    app: AppHandle,
    account_id: String,
) -> Result<CodexAccount, String> {
    let _ = codex_account::prepare_account_for_injection(&account_id).await?;

    // 切换账号（写入 auth.json）
    let account = codex_account::switch_account(&account_id)?;

    // 同步更新 Codex 默认实例的绑定账号（不同步到 Antigravity，因为账号体系不同）
    if let Err(e) = crate::modules::codex_instance::update_default_settings(
        Some(Some(account_id.clone())),
        None,
        Some(false),
    ) {
        logger::log_warn(&format!("更新 Codex 默认实例绑定账号失败: {}", e));
    } else {
        logger::log_info(&format!(
            "已同步更新 Codex 默认实例绑定账号: {}",
            account_id
        ));
    }

    let mut opencode_updated = false;
    match opencode_auth::replace_openai_entry_from_codex(&account) {
        Ok(()) => {
            opencode_updated = true;
        }
        Err(e) => {
            logger::log_warn(&format!("OpenCode auth.json 更新跳过: {}", e));
        }
    }

    let user_config = config::get_user_config();
    if user_config.opencode_sync_on_switch {
        if opencode_updated {
            if process::is_opencode_running() {
                if let Err(e) = process::close_opencode(20) {
                    logger::log_warn(&format!("OpenCode 关闭失败: {}", e));
                }
            } else {
                logger::log_info("OpenCode 未在运行，准备启动");
            }
            if let Err(e) = process::start_opencode_with_path(Some(&user_config.opencode_app_path))
            {
                logger::log_warn(&format!("OpenCode 启动失败: {}", e));
            }
        } else {
            logger::log_info("OpenCode 未更新 auth.json，跳过启动/重启");
        }
    } else {
        logger::log_info("已关闭 OpenCode 自动重启");
    }

    if user_config.codex_launch_on_switch {
        #[cfg(target_os = "macos")]
        if process::is_codex_running() {
            logger::log_info("检测到 Codex 正在运行，将按默认实例 PID 逻辑重启");
        }
        match crate::commands::codex_instance::codex_start_instance("__default__".to_string()).await
        {
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
    } else {
        logger::log_info("已关闭切换 Codex 时自动启动 Codex App");
    }

    let _ = crate::modules::tray::update_tray_menu(&app);
    Ok(account)
}

/// 删除 Codex 账号
#[tauri::command]
pub fn delete_codex_account(account_id: String) -> Result<(), String> {
    codex_account::remove_account(&account_id)
}

/// 批量删除 Codex 账号
#[tauri::command]
pub fn delete_codex_accounts(account_ids: Vec<String>) -> Result<(), String> {
    codex_account::remove_accounts(&account_ids)
}

/// 从本地 auth.json 导入账号
#[tauri::command]
pub fn import_codex_from_local() -> Result<CodexAccount, String> {
    codex_account::import_from_local()
}

/// 从 JSON 字符串导入账号
#[tauri::command]
pub fn import_codex_from_json(json_content: String) -> Result<Vec<CodexAccount>, String> {
    codex_account::import_from_json(&json_content)
}

/// 导出 Codex 账号
#[tauri::command]
pub fn export_codex_accounts(account_ids: Vec<String>) -> Result<String, String> {
    codex_account::export_accounts(&account_ids)
}

/// 刷新单个账号配额
#[tauri::command]
pub async fn refresh_codex_quota(app: AppHandle, account_id: String) -> Result<CodexQuota, String> {
    let result = codex_quota::refresh_account_quota(&account_id).await;
    if result.is_ok() {
        if let Err(e) = codex_account::run_quota_alert_if_needed() {
            logger::log_warn(&format!("[QuotaAlert][Codex] 预警检查失败: {}", e));
        }
        let _ = crate::modules::tray::update_tray_menu(&app);
    }
    result
}

#[tauri::command]
pub async fn refresh_current_codex_quota(app: AppHandle) -> Result<(), String> {
    let Some(account) = codex_account::get_current_account() else {
        return Err("未找到当前 Codex 账号".to_string());
    };
    let result = codex_quota::refresh_account_quota(&account.id).await;
    if result.is_ok() {
        if let Err(e) = codex_account::run_quota_alert_if_needed() {
            logger::log_warn(&format!(
                "[QuotaAlert][Codex] 当前账号刷新后预警检查失败: {}",
                e
            ));
        }
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
    let results = codex_quota::refresh_all_quotas().await?;
    let success_count = results.iter().filter(|(_, r)| r.is_ok()).count();
    if success_count > 0 {
        if let Err(e) = codex_account::run_quota_alert_if_needed() {
            logger::log_warn(&format!(
                "[QuotaAlert][Codex] 全量刷新后预警检查失败: {}",
                e
            ));
        }
    }
    let _ = crate::modules::tray::update_tray_menu(&app);
    Ok(success_count as i32)
}

async fn save_codex_oauth_tokens(tokens: CodexTokens) -> Result<CodexAccount, String> {
    let account = codex_account::upsert_account(tokens)?;

    if let Err(e) = codex_quota::refresh_account_quota(&account.id).await {
        logger::log_error(&format!("刷新配额失败: {}", e));
    }

    let loaded =
        codex_account::load_account(&account.id).ok_or_else(|| "账号保存后无法读取".to_string())?;
    logger::log_info(&format!(
        "Codex OAuth 账号已保存: account_id={}, email={}",
        loaded.id, loaded.email
    ));
    Ok(loaded)
}

/// OAuth：开始登录（返回 loginId + authUrl）
#[tauri::command]
pub async fn codex_oauth_login_start(
    app_handle: AppHandle,
) -> Result<codex_oauth::CodexOAuthLoginStartResponse, String> {
    logger::log_info("Codex OAuth start 命令触发");
    let response = codex_oauth::start_oauth_login(app_handle).await?;
    logger::log_info(&format!(
        "Codex OAuth start 命令成功: login_id={}",
        response.login_id
    ));
    Ok(response)
}

/// OAuth：浏览器授权完成后按 loginId 完成登录
#[tauri::command]
pub async fn codex_oauth_login_completed(login_id: String) -> Result<CodexAccount, String> {
    let started_at_ms = chrono::Utc::now().timestamp_millis();
    logger::log_info(&format!(
        "Codex OAuth completed 命令开始: login_id={}, started_at_ms={}",
        login_id, started_at_ms
    ));
    let tokens = match codex_oauth::complete_oauth_login(&login_id).await {
        Ok(tokens) => tokens,
        Err(e) => {
            logger::log_error(&format!(
                "Codex OAuth completed 命令失败: login_id={}, duration_ms={}, error={}",
                login_id,
                chrono::Utc::now().timestamp_millis() - started_at_ms,
                e
            ));
            return Err(e);
        }
    };
    let account = save_codex_oauth_tokens(tokens).await?;
    logger::log_info(&format!(
        "Codex OAuth completed 命令成功: login_id={}, duration_ms={}, account_id={}, account_email={}",
        login_id,
        chrono::Utc::now().timestamp_millis() - started_at_ms,
        account.id,
        account.email
    ));
    Ok(account)
}

/// OAuth：按 loginId 取消登录（login_id 为空时取消当前流程）
#[tauri::command]
pub fn codex_oauth_login_cancel(login_id: Option<String>) -> Result<(), String> {
    logger::log_info(&format!(
        "Codex OAuth cancel 命令触发: login_id={}",
        login_id.as_deref().unwrap_or("<none>")
    ));
    let result = codex_oauth::cancel_oauth_flow_for(login_id.as_deref());
    logger::log_info(&format!(
        "Codex OAuth cancel 命令返回: {:?}",
        result.as_ref().map(|_| "ok").map_err(|e| e)
    ));
    result
}

/// 通过 Token 添加账号
#[tauri::command]
pub async fn add_codex_account_with_token(
    id_token: String,
    access_token: String,
    refresh_token: Option<String>,
) -> Result<CodexAccount, String> {
    let tokens = CodexTokens {
        id_token,
        access_token,
        refresh_token,
    };

    let account = codex_account::upsert_account(tokens)?;

    // 刷新配额
    if let Err(e) = codex_quota::refresh_account_quota(&account.id).await {
        logger::log_error(&format!("刷新配额失败: {}", e));
    }

    codex_account::load_account(&account.id).ok_or_else(|| "账号保存后无法读取".to_string())
}

/// 检查 Codex OAuth 端口是否被占用
#[tauri::command]
pub async fn update_codex_account_tags(
    account_id: String,
    tags: Vec<String>,
) -> Result<CodexAccount, String> {
    codex_account::update_account_tags(&account_id, tags)
}

#[tauri::command]
pub fn is_codex_oauth_port_in_use() -> Result<bool, String> {
    let port = codex_oauth::get_callback_port();
    process::is_port_in_use(port)
}

/// 关闭占用 Codex OAuth 端口的进程
#[tauri::command]
pub fn close_codex_oauth_port() -> Result<u32, String> {
    let port = codex_oauth::get_callback_port();
    let killed = process::kill_port_processes(port)?;
    Ok(killed as u32)
}
