use crate::models::InstanceProfileView;
use crate::modules;
use serde_json::json;

#[tauri::command]
pub async fn antigravity_legacy_get_instance_defaults(
) -> Result<modules::instance::InstanceDefaults, String> {
    modules::platform_adapter::call_antigravity("instance.getDefaults", json!({}))
}

#[tauri::command]
pub async fn antigravity_legacy_list_instances() -> Result<Vec<InstanceProfileView>, String> {
    modules::platform_adapter::call_antigravity("instance.list", json!({}))
}

#[tauri::command]
pub async fn antigravity_legacy_create_instance(
    name: String,
    user_data_dir: String,
    extra_args: Option<String>,
    bind_account_id: Option<String>,
    copy_source_instance_id: Option<String>,
    init_mode: Option<String>,
) -> Result<InstanceProfileView, String> {
    modules::platform_adapter::call_antigravity(
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
pub async fn antigravity_legacy_update_instance(
    instance_id: String,
    name: Option<String>,
    extra_args: Option<String>,
    bind_account_id: Option<Option<String>>,
    follow_local_account: Option<bool>,
) -> Result<InstanceProfileView, String> {
    modules::platform_adapter::call_antigravity(
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
pub async fn antigravity_legacy_delete_instance(instance_id: String) -> Result<(), String> {
    modules::platform_adapter::call_antigravity(
        "instance.delete",
        json!({ "instanceId": instance_id }),
    )
}

#[tauri::command]
pub async fn antigravity_legacy_start_instance(
    instance_id: String,
) -> Result<InstanceProfileView, String> {
    modules::platform_adapter::call_antigravity(
        "instance.start",
        json!({ "instanceId": instance_id }),
    )
}

#[tauri::command]
pub async fn antigravity_legacy_stop_instance(
    instance_id: String,
) -> Result<InstanceProfileView, String> {
    modules::platform_adapter::call_antigravity(
        "instance.stop",
        json!({ "instanceId": instance_id }),
    )
}

#[tauri::command]
pub async fn antigravity_legacy_close_all_instances() -> Result<(), String> {
    modules::platform_adapter::call_antigravity("instance.closeAll", json!({}))
}

#[tauri::command]
pub async fn antigravity_legacy_open_instance_window(instance_id: String) -> Result<(), String> {
    modules::platform_adapter::call_antigravity(
        "instance.openWindow",
        json!({ "instanceId": instance_id }),
    )
}
