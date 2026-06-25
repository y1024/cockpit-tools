use serde::{Deserialize, Serialize};

use crate::modules;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaAlertPayload {
    pub platform: String,
    pub current_account_id: String,
    pub current_email: String,
    pub threshold: i32,
    pub threshold_display: Option<String>,
    pub lowest_percentage: i32,
    pub low_models: Vec<String>,
    pub recommended_account_id: Option<String>,
    pub recommended_email: Option<String>,
    pub triggered_at: i64,
}

fn build_quota_alert_notification_text(payload: &QuotaAlertPayload) -> (String, String) {
    let locale = crate::modules::config::get_user_config().language;
    let threshold_text = payload.threshold.to_string();
    let lowest_text = payload.lowest_percentage.to_string();
    let model_text = if payload.low_models.is_empty() {
        modules::i18n::translate(&locale, "quotaAlert.modal.unknownModel", &[])
    } else {
        payload.low_models.join(", ")
    };

    let platform_label = match payload.platform.as_str() {
        "codex" => "Codex",
        "github_copilot" => "GitHub Copilot",
        "windsurf" => "Windsurf",
        "kiro" => "Kiro",
        "cursor" => "Cursor",
        "gemini" => "Gemini Cli",
        "codebuddy" => "CodeBuddy",
        "zed" => "Zed",
        _ => "Antigravity IDE",
    };
    let title = format!(
        "{} {}",
        platform_label,
        modules::i18n::translate(&locale, "quotaAlert.modal.title", &[])
    );
    let mut body = modules::i18n::translate(
        &locale,
        "quotaAlert.bannerText",
        &[
            ("email", payload.current_email.as_str()),
            ("threshold", threshold_text.as_str()),
            ("lowest", lowest_text.as_str()),
            ("models", model_text.as_str()),
        ],
    );
    if let Some(email) = payload.recommended_email.as_ref() {
        let recommended_label =
            modules::i18n::translate(&locale, "quotaAlert.modal.recommended", &[]);
        body.push_str(" · ");
        body.push_str(&format!("{}: {}", recommended_label, email));
    }
    (title, body)
}

pub fn emit_quota_alert(app_handle: &tauri::AppHandle, payload: &QuotaAlertPayload) {
    use tauri::Emitter;
    let _ = app_handle.emit("quota:alert", payload);
}

#[cfg(not(target_os = "macos"))]
pub fn send_quota_alert_native_notification(payload: &QuotaAlertPayload) {
    let Some(app_handle) = crate::get_app_handle() else {
        return;
    };

    use tauri_plugin_notification::NotificationExt;

    let (title, body) = build_quota_alert_notification_text(payload);

    if let Err(e) = app_handle
        .notification()
        .builder()
        .title(&title)
        .body(body)
        .show()
    {
        modules::logger::log_warn(&format!("[QuotaAlert] 原生通知发送失败: {}", e));
    }
}

#[cfg(target_os = "macos")]
pub fn send_quota_alert_native_notification(payload: &QuotaAlertPayload) {
    let Some(app_handle) = crate::get_app_handle() else {
        return;
    };
    let bundle_identifier = app_handle.config().identifier.to_string();
    let (title, body) = build_quota_alert_notification_text(payload);

    std::thread::spawn(move || {
        let mut notification = mac_notification_sys::Notification::new();
        // Fire-and-forget on macOS. Waiting for clicks keeps a dedicated run loop alive
        // inside mac-notification-sys, which can cause persistent background energy usage.
        notification
            .title(title.as_str())
            .message(body.as_str())
            .wait_for_click(false)
            .asynchronous(true);

        if let Err(e) = mac_notification_sys::set_application(&bundle_identifier) {
            modules::logger::log_warn(&format!("[QuotaAlert] 设置通知应用标识失败: {}", e));
        }

        if let Err(e) = notification.send() {
            modules::logger::log_warn(&format!("[QuotaAlert] 原生通知发送失败: {}", e));
        }
    });
}

pub fn dispatch_quota_alert(payload: &QuotaAlertPayload) {
    modules::logger::log_warn(&format!(
        "[QuotaAlert] 触发配额预警: platform={}, current_id={}, threshold={}%, lowest={}%",
        payload.platform, payload.current_account_id, payload.threshold, payload.lowest_percentage
    ));

    if let Some(app_handle) = crate::get_app_handle() {
        emit_quota_alert(app_handle, payload);
    }
    send_quota_alert_native_notification(payload);
}
