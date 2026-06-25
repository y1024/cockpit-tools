#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::process::Command;

use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::models::codex::{
    CodexAppSpeed, CodexInstanceTargetThreadSyncSummary, CodexInstanceThreadSyncSummary,
    CodexQuickConfig, CodexSessionRecord, CodexSessionRestoreSummary, CodexSessionTokenStats,
    CodexSessionTrashSummary, CodexSessionVisibilityRepairInstanceList,
    CodexSessionVisibilityRepairMode, CodexSessionVisibilityRepairProviderList,
    CodexSessionVisibilityRepairSummary, CodexTrashedSessionRecord,
};
use crate::models::InstanceLaunchMode;
use crate::modules;

const DEFAULT_INSTANCE_ID: &str = "__default__";
const CODEX_INSTANCE_FAST_READ_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexLaunchCredentialChange {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexInstanceLaunchInfo {
    pub instance_id: String,
    pub user_data_dir: String,
    pub launch_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexInstanceProfileView {
    pub id: String,
    pub name: String,
    pub user_data_dir: String,
    pub working_dir: Option<String>,
    pub extra_args: String,
    pub bind_account_id: Option<String>,
    pub launch_mode: InstanceLaunchMode,
    pub app_speed: CodexAppSpeed,
    pub created_at: i64,
    pub last_launched_at: Option<i64>,
    pub last_pid: Option<u32>,
    pub running: bool,
    pub initialized: bool,
    pub is_default: bool,
    pub follow_local_account: bool,
    pub auto_sync_threads: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codex_launch_credential_change: Option<CodexLaunchCredentialChange>,
}

#[cfg(target_os = "macos")]
fn escape_applescript(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[tauri::command]
pub async fn codex_get_instance_defaults() -> Result<modules::instance::InstanceDefaults, String> {
    modules::platform_adapter::call_codex("instances.defaults", serde_json::json!({}))
}

#[tauri::command]
pub async fn codex_list_instances() -> Result<Vec<CodexInstanceProfileView>, String> {
    modules::platform_adapter::call_codex_with_timeout(
        "instances.list",
        serde_json::json!({}),
        CODEX_INSTANCE_FAST_READ_TIMEOUT,
    )
}

#[tauri::command]
pub async fn codex_get_instance_quick_config(
    instance_id: String,
) -> Result<CodexQuickConfig, String> {
    modules::platform_adapter::call_codex(
        "instances.quickConfig.get",
        serde_json::json!({ "instanceId": instance_id }),
    )
}

#[tauri::command]
pub async fn codex_save_instance_quick_config(
    instance_id: String,
    model_context_window: Option<i64>,
    auto_compact_token_limit: Option<i64>,
) -> Result<CodexQuickConfig, String> {
    modules::platform_adapter::call_codex(
        "instances.quickConfig.save",
        serde_json::json!({
            "instanceId": instance_id,
            "modelContextWindow": model_context_window,
            "autoCompactTokenLimit": auto_compact_token_limit,
        }),
    )
}

#[tauri::command]
pub async fn codex_open_instance_config_toml(
    app: AppHandle,
    instance_id: String,
) -> Result<(), String> {
    let path: String = modules::platform_adapter::call_codex(
        "instances.configPath",
        serde_json::json!({ "instanceId": instance_id }),
    )?;
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|e| format!("打开实例 config.toml 失败: {}", e))
}

#[tauri::command]
pub async fn codex_sync_threads_across_instances() -> Result<CodexInstanceThreadSyncSummary, String>
{
    modules::platform_adapter::call_codex(
        "sessions.syncThreadsAcrossInstances",
        serde_json::json!({}),
    )
}

#[tauri::command]
pub async fn codex_sync_sessions_to_instance(
    session_ids: Vec<String>,
    target_instance_id: String,
) -> Result<CodexInstanceTargetThreadSyncSummary, String> {
    modules::platform_adapter::call_codex(
        "sessions.syncToInstance",
        serde_json::json!({
            "sessionIds": session_ids,
            "targetInstanceId": target_instance_id,
        }),
    )
}

#[tauri::command]
pub async fn codex_repair_session_visibility_across_instances(
    mode: Option<CodexSessionVisibilityRepairMode>,
    run_id: Option<String>,
    target_provider: Option<String>,
    target_instance_id: Option<String>,
    repair_instance_ids: Option<Vec<String>>,
    session_ids: Option<Vec<String>>,
) -> Result<CodexSessionVisibilityRepairSummary, String> {
    modules::platform_adapter::call_codex(
        "sessions.visibilityRepair.run",
        serde_json::json!({
            "mode": mode,
            "runId": run_id,
            "targetProvider": target_provider,
            "targetInstanceId": target_instance_id,
            "repairInstanceIds": repair_instance_ids,
            "sessionIds": session_ids,
        }),
    )
}

#[tauri::command]
pub async fn codex_list_session_visibility_repair_providers(
) -> Result<CodexSessionVisibilityRepairProviderList, String> {
    modules::platform_adapter::call_codex(
        "sessions.visibilityRepairProviders.list",
        serde_json::json!({}),
    )
}

#[tauri::command]
pub async fn codex_list_session_visibility_repair_instances(
) -> Result<CodexSessionVisibilityRepairInstanceList, String> {
    modules::platform_adapter::call_codex(
        "sessions.visibilityRepairInstances.list",
        serde_json::json!({}),
    )
}

#[tauri::command]
pub async fn codex_list_sessions_across_instances(
    title_query: Option<String>,
    content_query: Option<String>,
) -> Result<Vec<CodexSessionRecord>, String> {
    modules::platform_adapter::call_codex(
        "sessions.list",
        serde_json::json!({
            "titleQuery": title_query,
            "contentQuery": content_query,
        }),
    )
}

#[tauri::command]
pub async fn codex_get_session_token_stats_across_instances(
    session_ids: Vec<String>,
) -> Result<Vec<CodexSessionTokenStats>, String> {
    modules::platform_adapter::call_codex(
        "sessions.tokenStats",
        serde_json::json!({ "sessionIds": session_ids }),
    )
}

#[tauri::command]
pub async fn codex_move_sessions_to_trash_across_instances(
    session_ids: Vec<String>,
) -> Result<CodexSessionTrashSummary, String> {
    modules::platform_adapter::call_codex(
        "sessions.moveToTrash",
        serde_json::json!({ "sessionIds": session_ids }),
    )
}

#[tauri::command]
pub async fn codex_list_trashed_sessions_across_instances(
) -> Result<Vec<CodexTrashedSessionRecord>, String> {
    modules::platform_adapter::call_codex("sessions.listTrash", serde_json::json!({}))
}

#[tauri::command]
pub async fn codex_restore_sessions_from_trash_across_instances(
    session_ids: Vec<String>,
) -> Result<CodexSessionRestoreSummary, String> {
    modules::platform_adapter::call_codex(
        "sessions.restoreFromTrash",
        serde_json::json!({ "sessionIds": session_ids }),
    )
}

#[tauri::command]
pub async fn codex_create_instance(
    name: String,
    user_data_dir: String,
    working_dir: Option<String>,
    extra_args: Option<String>,
    bind_account_id: Option<String>,
    copy_source_instance_id: Option<String>,
    init_mode: Option<String>,
    launch_mode: Option<InstanceLaunchMode>,
    app_speed: Option<CodexAppSpeed>,
) -> Result<CodexInstanceProfileView, String> {
    modules::platform_adapter::call_codex(
        "instances.create",
        serde_json::json!({
            "name": name,
            "userDataDir": user_data_dir,
            "workingDir": working_dir,
            "extraArgs": extra_args,
            "bindAccountId": bind_account_id,
            "copySourceInstanceId": copy_source_instance_id,
            "initMode": init_mode,
            "launchMode": launch_mode,
            "appSpeed": app_speed,
        }),
    )
}

#[tauri::command]
pub async fn codex_update_instance(
    instance_id: String,
    name: Option<String>,
    working_dir: Option<String>,
    extra_args: Option<String>,
    bind_account_id: Option<Option<String>>,
    follow_local_account: Option<bool>,
    launch_mode: Option<InstanceLaunchMode>,
    app_speed: Option<CodexAppSpeed>,
    auto_sync_threads: Option<bool>,
) -> Result<CodexInstanceProfileView, String> {
    let bind_account_id_set = bind_account_id.is_some();
    modules::platform_adapter::call_codex(
        "instances.update",
        serde_json::json!({
            "instanceId": instance_id,
            "name": name,
            "workingDir": working_dir,
            "extraArgs": extra_args,
            "bindAccountId": bind_account_id.flatten(),
            "bindAccountIdSet": bind_account_id_set,
            "followLocalAccount": follow_local_account,
            "launchMode": launch_mode,
            "appSpeed": app_speed,
            "autoSyncThreads": auto_sync_threads,
        }),
    )
}

#[tauri::command]
pub async fn codex_delete_instance(instance_id: String) -> Result<(), String> {
    modules::platform_adapter::call_codex(
        "instances.delete",
        serde_json::json!({ "instanceId": instance_id }),
    )
}

pub(crate) async fn codex_start_default_with_prepared_profile(
) -> Result<CodexInstanceProfileView, String> {
    modules::platform_adapter::call_codex(
        "instances.start",
        serde_json::json!({
            "instanceId": DEFAULT_INSTANCE_ID,
            "skipDefaultBindAccountInjection": true,
        }),
    )
}

#[tauri::command]
pub async fn codex_start_instance(instance_id: String) -> Result<CodexInstanceProfileView, String> {
    modules::platform_adapter::call_codex(
        "instances.start",
        serde_json::json!({
            "instanceId": instance_id,
            "skipDefaultBindAccountInjection": false,
        }),
    )
}

#[tauri::command]
pub async fn codex_stop_instance(instance_id: String) -> Result<CodexInstanceProfileView, String> {
    modules::platform_adapter::call_codex(
        "instances.stop",
        serde_json::json!({ "instanceId": instance_id }),
    )
}

#[tauri::command]
pub async fn codex_close_all_instances() -> Result<(), String> {
    modules::platform_adapter::call_codex_value("instances.closeAll", serde_json::json!({}))
        .map(|_| ())
}

#[tauri::command]
pub async fn codex_open_instance_window(instance_id: String) -> Result<(), String> {
    modules::platform_adapter::call_codex_value(
        "instances.window.open",
        serde_json::json!({ "instanceId": instance_id }),
    )
    .map(|_| ())
}

#[tauri::command]
pub async fn codex_get_instance_launch_command(
    instance_id: String,
) -> Result<CodexInstanceLaunchInfo, String> {
    modules::platform_adapter::call_codex(
        "instances.launchCommand.get",
        serde_json::json!({ "instanceId": instance_id }),
    )
}

#[tauri::command]
pub async fn codex_execute_instance_launch_command(
    instance_id: String,
    terminal: Option<String>,
) -> Result<String, String> {
    let launch_info: CodexInstanceLaunchInfo = modules::platform_adapter::call_codex(
        "instances.launchCommand.get",
        serde_json::json!({ "instanceId": instance_id }),
    )?;
    let command = launch_info.launch_command;

    #[cfg(target_os = "macos")]
    {
        let config = crate::modules::config::get_user_config();
        let terminal = terminal
            .unwrap_or(config.default_terminal)
            .trim()
            .to_string();
        let is_iterm = terminal.to_lowercase().contains("iterm");
        let is_terminal_app = terminal == "system" || terminal.is_empty() || terminal == "Terminal";
        let app_name = if is_terminal_app {
            "Terminal"
        } else {
            &terminal
        };

        let script = if is_iterm {
            format!(
                "tell application \"iTerm\"
                    activate
                    if not (exists window 1) then
                        create window with default profile
                        tell current session of current window
                            write text \"{}\"
                        end tell
                    else
                        tell current window
                            create tab with default profile
                            tell current session
                                write text \"{}\"
                            end tell
                        end tell
                    end if
                end tell",
                escape_applescript(&command),
                escape_applescript(&command)
            )
        } else if is_terminal_app {
            format!(
                "tell application \"Terminal\"
                    activate
                    do script \"{}\"
                end tell",
                escape_applescript(&command)
            )
        } else {
            return Err(format!(
                "当前终端暂不支持直接执行：{}。请改用 Terminal 或 iTerm2。",
                terminal
            ));
        };

        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| format!("打开终端失败 ({}): {}", app_name, e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("终端执行失败: {}", stderr.trim()));
        }
        return Ok(format!("已在 {} 执行 Codex CLI 命令", app_name));
    }

    #[cfg(target_os = "windows")]
    {
        let config = crate::modules::config::get_user_config();
        let terminal = terminal
            .unwrap_or(config.default_terminal)
            .trim()
            .to_string();

        let mut cmd = if terminal == "pwsh" {
            let mut command_process = Command::new("pwsh");
            command_process.args(["-NoExit", "-Command", &command]);
            command_process
        } else if terminal == "wt" {
            let mut command_process = Command::new("wt");
            command_process.args(["powershell", "-NoExit", "-Command", &command]);
            command_process
        } else if terminal == "cmd" {
            let mut command_process = Command::new("cmd");
            command_process.args([
                "/C",
                "start",
                "",
                "powershell",
                "-NoExit",
                "-Command",
                &command,
            ]);
            command_process
        } else {
            let mut command_process = Command::new("powershell");
            command_process.args(["-NoExit", "-Command", &command]);
            command_process
        };

        cmd.spawn().map_err(|e| format!("打开终端失败: {}", e))?;
        return Ok("已在终端执行 Codex CLI 命令".to_string());
    }

    #[allow(unreachable_code)]
    Err("Codex CLI 终端执行仅支持 macOS 和 Windows".to_string())
}
