use crate::models::InstanceProfileView;
use crate::modules;
use serde_json::json;

#[tauri::command]
pub async fn get_instance_defaults() -> Result<modules::instance::InstanceDefaults, String> {
    modules::platform_adapter::call_antigravity_ide("instance.getDefaults", json!({}))
}

#[tauri::command]
pub async fn list_instances() -> Result<Vec<InstanceProfileView>, String> {
    modules::platform_adapter::call_antigravity_ide("instance.list", json!({}))
}

#[tauri::command]
pub async fn create_instance(
    name: String,
    user_data_dir: String,
    extra_args: Option<String>,
    bind_account_id: Option<String>,
    copy_source_instance_id: Option<String>,
    init_mode: Option<String>,
) -> Result<InstanceProfileView, String> {
    modules::platform_adapter::call_antigravity_ide(
        "instance.create",
        json!({
            "name": name,
            "userDataDir": user_data_dir,
            "extraArgs": extra_args,
            "bindAccountId": bind_account_id,
            "copySourceInstanceId": copy_source_instance_id,
            "initMode": init_mode,
        }),
    )
}

#[tauri::command]
pub async fn update_instance(
    instance_id: String,
    name: Option<String>,
    extra_args: Option<String>,
    bind_account_id: Option<Option<String>>,
    follow_local_account: Option<bool>,
) -> Result<InstanceProfileView, String> {
    modules::platform_adapter::call_antigravity_ide(
        "instance.update",
        json!({
            "instanceId": instance_id,
            "name": name,
            "extraArgs": extra_args,
            "bindAccountId": bind_account_id,
            "followLocalAccount": follow_local_account,
        }),
    )
}

#[tauri::command]
pub async fn delete_instance(instance_id: String) -> Result<(), String> {
    modules::platform_adapter::call_antigravity_ide(
        "instance.delete",
        json!({ "instanceId": instance_id }),
    )
}

#[tauri::command]
pub async fn start_instance(instance_id: String) -> Result<InstanceProfileView, String> {
    modules::platform_adapter::call_antigravity_ide(
        "instance.start",
        json!({ "instanceId": instance_id }),
    )
}

#[tauri::command]
pub async fn stop_instance(instance_id: String) -> Result<InstanceProfileView, String> {
    modules::platform_adapter::call_antigravity_ide(
        "instance.stop",
        json!({ "instanceId": instance_id }),
    )
}

#[tauri::command]
pub async fn close_all_instances() -> Result<(), String> {
    modules::platform_adapter::call_antigravity_ide("instance.closeAll", json!({}))
}

#[tauri::command]
pub async fn open_instance_window(instance_id: String) -> Result<(), String> {
    modules::platform_adapter::call_antigravity_ide(
        "instance.openWindow",
        json!({ "instanceId": instance_id }),
    )
}
