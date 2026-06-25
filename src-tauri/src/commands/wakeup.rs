use crate::modules::{self, platform_adapter};
use tauri::AppHandle;

#[tauri::command]
pub fn wakeup_ensure_runtime_ready(
    official_ls_version_mode: Option<String>,
) -> Result<Option<String>, String> {
    platform_adapter::call_antigravity_series(
        "wakeup.runtime.ensureReady",
        serde_json::json!({ "officialLsVersionMode": official_ls_version_mode }),
    )
}

#[tauri::command]
pub async fn trigger_wakeup(
    account_id: String,
    model: String,
    prompt: Option<String>,
    max_output_tokens: Option<u32>,
    cancel_scope_id: Option<String>,
    official_ls_version_mode: Option<String>,
) -> Result<modules::wakeup::WakeupResponse, String> {
    platform_adapter::call_antigravity_series(
        "wakeup.trigger",
        serde_json::json!({
            "accountId": account_id,
            "model": model,
            "prompt": prompt,
            "maxOutputTokens": max_output_tokens,
            "cancelScopeId": cancel_scope_id,
            "officialLsVersionMode": official_ls_version_mode,
        }),
    )
}

#[tauri::command]
pub async fn fetch_available_models() -> Result<Vec<modules::wakeup::AvailableModel>, String> {
    platform_adapter::call_antigravity_series("wakeup.fetchAvailableModels", serde_json::json!({}))
}

#[tauri::command]
pub fn wakeup_validate_crontab(expr: String) -> Result<(), String> {
    platform_adapter::call_antigravity_series(
        "wakeup.crontab.validate",
        serde_json::json!({ "expr": expr }),
    )
}

#[tauri::command]
pub async fn wakeup_sync_state(
    _app: AppHandle,
    enabled: bool,
    tasks: Vec<modules::wakeup_scheduler::WakeupTaskInput>,
    official_ls_version_mode: Option<String>,
    run_startup_tasks: Option<bool>,
) -> Result<(), String> {
    platform_adapter::call_antigravity_series(
        "wakeup.scheduler.syncState",
        serde_json::json!({
            "enabled": enabled,
            "tasks": tasks,
            "officialLsVersionMode": official_ls_version_mode,
            "runStartupTasks": run_startup_tasks,
        }),
    )
}

#[tauri::command]
pub async fn wakeup_run_enabled_tasks(
    _app: AppHandle,
    trigger_source: Option<String>,
    official_ls_version_mode: Option<String>,
) -> Result<u32, String> {
    platform_adapter::call_antigravity_series(
        "wakeup.scheduler.runEnabledTasks",
        serde_json::json!({
            "triggerSource": trigger_source,
            "officialLsVersionMode": official_ls_version_mode,
        }),
    )
}

#[tauri::command]
pub fn wakeup_load_history() -> Result<Vec<modules::wakeup_history::WakeupHistoryItem>, String> {
    platform_adapter::call_antigravity_series("wakeup.sharedHistory.load", serde_json::json!({}))
}

#[tauri::command]
pub fn wakeup_add_history(
    items: Vec<modules::wakeup_history::WakeupHistoryItem>,
) -> Result<(), String> {
    platform_adapter::call_antigravity_series(
        "wakeup.sharedHistory.add",
        serde_json::json!({ "items": items }),
    )
}

#[tauri::command]
pub fn wakeup_clear_history() -> Result<(), String> {
    platform_adapter::call_antigravity_series("wakeup.sharedHistory.clear", serde_json::json!({}))
}

#[tauri::command]
pub fn wakeup_cancel_scope(cancel_scope_id: String) -> Result<(), String> {
    platform_adapter::call_antigravity_series(
        "wakeup.sharedScope.cancel",
        serde_json::json!({ "cancelScopeId": cancel_scope_id }),
    )
}

#[tauri::command]
pub fn wakeup_release_scope(cancel_scope_id: String) -> Result<(), String> {
    platform_adapter::call_antigravity_series(
        "wakeup.sharedScope.release",
        serde_json::json!({ "cancelScopeId": cancel_scope_id }),
    )
}

#[tauri::command]
pub fn wakeup_verification_load_state(
) -> Result<Vec<modules::wakeup_verification::WakeupVerificationStateItem>, String> {
    platform_adapter::call_antigravity_series(
        "wakeup.verification.loadState",
        serde_json::json!({}),
    )
}

#[tauri::command]
pub fn wakeup_verification_load_history(
) -> Result<Vec<modules::wakeup_verification::WakeupVerificationBatchHistoryItem>, String> {
    platform_adapter::call_antigravity_series(
        "wakeup.verification.loadHistory",
        serde_json::json!({}),
    )
}

#[tauri::command]
pub fn wakeup_verification_delete_history(batch_ids: Vec<String>) -> Result<usize, String> {
    platform_adapter::call_antigravity_series(
        "wakeup.verification.deleteHistory",
        serde_json::json!({ "batchIds": batch_ids }),
    )
}

#[tauri::command]
pub async fn wakeup_verification_run_batch(
    _app: AppHandle,
    account_ids: Vec<String>,
    model: String,
    prompt: Option<String>,
    max_output_tokens: Option<u32>,
    official_ls_version_mode: Option<String>,
) -> Result<modules::wakeup_verification::WakeupVerificationBatchResult, String> {
    platform_adapter::call_antigravity_series(
        "wakeup.verification.runBatch",
        serde_json::json!({
            "accountIds": account_ids,
            "model": model,
            "prompt": prompt,
            "maxOutputTokens": max_output_tokens,
            "officialLsVersionMode": official_ls_version_mode,
        }),
    )
}

#[tauri::command]
pub fn wakeup_set_official_ls_version_mode(mode: Option<String>) -> Result<(), String> {
    platform_adapter::call_antigravity_series(
        "wakeup.setOfficialLsVersionMode",
        serde_json::json!({ "officialLsVersionMode": mode }),
    )
}

#[tauri::command]
pub async fn confirm_wakeup_task(_app: AppHandle, task_id: String) -> Result<(), String> {
    platform_adapter::call_antigravity_series(
        "wakeup.scheduler.confirmTask",
        serde_json::json!({ "taskId": task_id }),
    )
}

#[tauri::command]
pub async fn cancel_wakeup_task(task_id: String) -> Result<(), String> {
    platform_adapter::call_antigravity_series(
        "wakeup.scheduler.cancelTask",
        serde_json::json!({ "taskId": task_id }),
    )
}

#[tauri::command]
pub async fn check_wakeup_timeouts(_app: AppHandle) -> Result<(), String> {
    platform_adapter::call_antigravity_series(
        "wakeup.scheduler.checkTimeouts",
        serde_json::json!({}),
    )
}
