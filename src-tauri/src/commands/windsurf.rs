use tauri::AppHandle;

use crate::models::github_copilot::{GitHubCopilotAccount, GitHubCopilotOAuthStartResponse};

#[tauri::command]
pub fn list_windsurf_accounts() -> Result<Vec<GitHubCopilotAccount>, String> {
    crate::commands::github_copilot::list_github_copilot_accounts()
}

#[tauri::command]
pub fn delete_windsurf_account(account_id: String) -> Result<(), String> {
    crate::commands::github_copilot::delete_github_copilot_account(account_id)
}

#[tauri::command]
pub fn delete_windsurf_accounts(account_ids: Vec<String>) -> Result<(), String> {
    crate::commands::github_copilot::delete_github_copilot_accounts(account_ids)
}

#[tauri::command]
pub fn import_windsurf_from_json(json_content: String) -> Result<Vec<GitHubCopilotAccount>, String> {
    crate::commands::github_copilot::import_github_copilot_from_json(json_content)
}

#[tauri::command]
pub fn export_windsurf_accounts(account_ids: Vec<String>) -> Result<String, String> {
    crate::commands::github_copilot::export_github_copilot_accounts(account_ids)
}

#[tauri::command]
pub async fn refresh_windsurf_token(
    app: AppHandle,
    account_id: String,
) -> Result<GitHubCopilotAccount, String> {
    crate::commands::github_copilot::refresh_github_copilot_token(app, account_id).await
}

#[tauri::command]
pub async fn refresh_all_windsurf_tokens(app: AppHandle) -> Result<i32, String> {
    crate::commands::github_copilot::refresh_all_github_copilot_tokens(app).await
}

#[tauri::command]
pub async fn windsurf_oauth_login_start() -> Result<GitHubCopilotOAuthStartResponse, String> {
    crate::commands::github_copilot::github_copilot_oauth_login_start().await
}

#[tauri::command]
pub async fn windsurf_oauth_login_complete(login_id: String) -> Result<GitHubCopilotAccount, String> {
    crate::commands::github_copilot::github_copilot_oauth_login_complete(login_id).await
}

#[tauri::command]
pub fn windsurf_oauth_login_cancel(login_id: Option<String>) -> Result<(), String> {
    crate::commands::github_copilot::github_copilot_oauth_login_cancel(login_id)
}

#[tauri::command]
pub async fn add_windsurf_account_with_token(
    github_access_token: String,
) -> Result<GitHubCopilotAccount, String> {
    crate::commands::github_copilot::add_github_copilot_account_with_token(github_access_token).await
}

#[tauri::command]
pub async fn update_windsurf_account_tags(
    account_id: String,
    tags: Vec<String>,
) -> Result<GitHubCopilotAccount, String> {
    crate::commands::github_copilot::update_github_copilot_account_tags(account_id, tags).await
}

#[tauri::command]
pub fn get_windsurf_accounts_index_path() -> Result<String, String> {
    crate::commands::github_copilot::get_github_copilot_accounts_index_path()
}

#[tauri::command]
pub async fn inject_windsurf_to_vscode(account_id: String) -> Result<String, String> {
    crate::commands::github_copilot::inject_github_copilot_to_vscode(account_id).await
}
