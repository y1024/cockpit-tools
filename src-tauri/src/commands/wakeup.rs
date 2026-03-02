use crate::modules;
use tauri::AppHandle;

#[tauri::command]
pub fn wakeup_ensure_runtime_ready() -> Result<Option<String>, String> {
    modules::wakeup::ensure_wakeup_runtime_ready()
}

#[tauri::command]
pub async fn trigger_wakeup(
    account_id: String,
    model: String,
    prompt: Option<String>,
    max_output_tokens: Option<u32>,
) -> Result<modules::wakeup::WakeupResponse, String> {
    let final_prompt = prompt.unwrap_or_else(|| "hi".to_string());
    let final_tokens = max_output_tokens.unwrap_or(0);
    modules::wakeup::trigger_wakeup(&account_id, &model, &final_prompt, final_tokens).await
}

#[tauri::command]
pub async fn fetch_available_models() -> Result<Vec<modules::wakeup::AvailableModel>, String> {
    modules::wakeup::fetch_available_models().await
}

#[tauri::command]
pub async fn wakeup_sync_state(
    app: AppHandle,
    enabled: bool,
    tasks: Vec<modules::wakeup_scheduler::WakeupTaskInput>,
) -> Result<(), String> {
    modules::wakeup_scheduler::sync_state(enabled, tasks);
    modules::wakeup_scheduler::ensure_started(app);
    Ok(())
}

#[tauri::command]
pub fn wakeup_load_history() -> Result<Vec<modules::wakeup_history::WakeupHistoryItem>, String> {
    modules::wakeup_history::load_history()
}

#[tauri::command]
pub fn wakeup_add_history(
    items: Vec<modules::wakeup_history::WakeupHistoryItem>,
) -> Result<(), String> {
    modules::wakeup_history::add_history_items(items)
}

#[tauri::command]
pub fn wakeup_clear_history() -> Result<(), String> {
    modules::wakeup_history::clear_history()
}

#[tauri::command]
pub fn wakeup_verification_load_state(
) -> Result<Vec<modules::wakeup_verification::WakeupVerificationStateItem>, String> {
    modules::wakeup_verification::build_display_state_for_all_accounts()
}

#[tauri::command]
pub fn wakeup_verification_load_history(
) -> Result<Vec<modules::wakeup_verification::WakeupVerificationBatchHistoryItem>, String> {
    modules::wakeup_verification::load_history()
}

#[tauri::command]
pub fn wakeup_verification_delete_history(batch_ids: Vec<String>) -> Result<usize, String> {
    modules::wakeup_verification::delete_history(batch_ids)
}

#[tauri::command]
pub async fn wakeup_verification_run_batch(
    app: AppHandle,
    account_ids: Vec<String>,
    model: String,
    prompt: Option<String>,
    max_output_tokens: Option<u32>,
) -> Result<modules::wakeup_verification::WakeupVerificationBatchResult, String> {
    let final_prompt = prompt.unwrap_or_else(|| "hi".to_string());
    let final_tokens = max_output_tokens.unwrap_or(0);
    modules::wakeup_verification::run_batch(&app, account_ids, &model, &final_prompt, final_tokens)
        .await
}
