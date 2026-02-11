use crate::models::InstanceProfileView;

#[tauri::command]
pub async fn windsurf_get_instance_defaults() -> Result<crate::modules::instance::InstanceDefaults, String> {
    crate::commands::github_copilot_instance::github_copilot_get_instance_defaults().await
}

#[tauri::command]
pub async fn windsurf_list_instances() -> Result<Vec<InstanceProfileView>, String> {
    crate::commands::github_copilot_instance::github_copilot_list_instances().await
}

#[tauri::command]
pub async fn windsurf_create_instance(
    name: String,
    user_data_dir: String,
    extra_args: Option<String>,
    bind_account_id: Option<String>,
    copy_source_instance_id: Option<String>,
    init_mode: Option<String>,
) -> Result<InstanceProfileView, String> {
    crate::commands::github_copilot_instance::github_copilot_create_instance(
        name,
        user_data_dir,
        extra_args,
        bind_account_id,
        copy_source_instance_id,
        init_mode,
    )
    .await
}

#[tauri::command]
pub async fn windsurf_update_instance(
    instance_id: String,
    name: Option<String>,
    extra_args: Option<String>,
    bind_account_id: Option<Option<String>>,
    follow_local_account: Option<bool>,
) -> Result<InstanceProfileView, String> {
    crate::commands::github_copilot_instance::github_copilot_update_instance(
        instance_id,
        name,
        extra_args,
        bind_account_id,
        follow_local_account,
    )
    .await
}

#[tauri::command]
pub async fn windsurf_delete_instance(instance_id: String) -> Result<(), String> {
    crate::commands::github_copilot_instance::github_copilot_delete_instance(instance_id).await
}

#[tauri::command]
pub async fn windsurf_start_instance(instance_id: String) -> Result<InstanceProfileView, String> {
    crate::commands::github_copilot_instance::github_copilot_start_instance(instance_id).await
}

#[tauri::command]
pub async fn windsurf_stop_instance(instance_id: String) -> Result<InstanceProfileView, String> {
    crate::commands::github_copilot_instance::github_copilot_stop_instance(instance_id).await
}

#[tauri::command]
pub async fn windsurf_open_instance_window(instance_id: String) -> Result<(), String> {
    crate::commands::github_copilot_instance::github_copilot_open_instance_window(instance_id).await
}

#[tauri::command]
pub async fn windsurf_close_all_instances() -> Result<(), String> {
    crate::commands::github_copilot_instance::github_copilot_close_all_instances().await
}
