use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::modules;
use crate::modules::config::{
    self, CloseWindowBehavior, MinimizeWindowBehavior, UserConfig, DEFAULT_WS_PORT,
};
use crate::modules::websocket;

/// 网络服务配置（前端使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// WebSocket 是否启用
    pub ws_enabled: bool,
    /// 配置的端口
    pub ws_port: u16,
    /// 实际运行的端口（可能与配置不同）
    pub actual_port: Option<u16>,
    /// 默认端口
    pub default_port: u16,
}

/// 通用设置配置（前端使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// 界面语言
    pub language: String,
    /// 应用主题: "light", "dark", "system"
    pub theme: String,
    /// 界面缩放比例（WebView Zoom）
    pub ui_scale: f64,
    /// 自动刷新间隔（分钟），-1 表示禁用
    pub auto_refresh_minutes: i32,
    /// Codex 自动刷新间隔（分钟），-1 表示禁用
    pub codex_auto_refresh_minutes: i32,
    /// GitHub Copilot 自动刷新间隔（分钟），-1 表示禁用
    pub ghcp_auto_refresh_minutes: i32,
    /// Windsurf 自动刷新间隔（分钟），-1 表示禁用
    pub windsurf_auto_refresh_minutes: i32,
    /// Kiro 自动刷新间隔（分钟），-1 表示禁用
    pub kiro_auto_refresh_minutes: i32,
    /// Cursor 自动刷新间隔（分钟），-1 表示禁用
    pub cursor_auto_refresh_minutes: i32,
    /// Gemini 自动刷新间隔（分钟），-1 表示禁用
    pub gemini_auto_refresh_minutes: i32,
    /// CodeBuddy 自动刷新间隔（分钟），-1 表示禁用
    pub codebuddy_auto_refresh_minutes: i32,
    /// CodeBuddy CN 自动刷新间隔（分钟），-1 表示禁用
    pub codebuddy_cn_auto_refresh_minutes: i32,
    /// Qoder 自动刷新间隔（分钟），-1 表示禁用
    pub qoder_auto_refresh_minutes: i32,
    /// Trae 自动刷新间隔（分钟），-1 表示禁用
    pub trae_auto_refresh_minutes: i32,
    /// 窗口关闭行为: "ask", "minimize", "quit"
    pub close_behavior: String,
    /// 窗口最小化行为（macOS）: "dock_and_tray", "tray_only"
    pub minimize_behavior: String,
    /// 是否隐藏 Dock 图标（macOS）
    pub hide_dock_icon: bool,
    /// OpenCode 启动路径（为空则使用默认路径）
    pub opencode_app_path: String,
    /// Antigravity 启动路径（为空则使用默认路径）
    pub antigravity_app_path: String,
    /// Codex 启动路径（为空则使用默认路径）
    pub codex_app_path: String,
    /// VS Code 启动路径（为空则使用默认路径）
    pub vscode_app_path: String,
    /// Windsurf 启动路径（为空则使用默认路径）
    pub windsurf_app_path: String,
    /// Kiro 启动路径（为空则使用默认路径）
    pub kiro_app_path: String,
    /// Cursor 启动路径（为空则使用默认路径）
    pub cursor_app_path: String,
    /// CodeBuddy 启动路径（为空则使用默认路径）
    pub codebuddy_app_path: String,
    /// CodeBuddy CN 启动路径（为空则使用默认路径）
    pub codebuddy_cn_app_path: String,
    /// Qoder 启动路径（为空则使用默认路径）
    pub qoder_app_path: String,
    /// Trae 启动路径（为空则使用默认路径）
    pub trae_app_path: String,
    /// 切换 Codex 时是否自动重启 OpenCode
    pub opencode_sync_on_switch: bool,
    /// 切换 Codex 时是否覆盖 OpenCode 登录信息
    pub opencode_auth_overwrite_on_switch: bool,
    /// 切换 Codex 时是否自动启动/重启 Codex App
    pub codex_launch_on_switch: bool,
    /// 是否启用自动切号
    pub auto_switch_enabled: bool,
    /// 自动切号阈值（百分比）
    pub auto_switch_threshold: i32,
    /// 是否启用配额预警通知
    pub quota_alert_enabled: bool,
    /// 配额预警阈值（百分比）
    pub quota_alert_threshold: i32,
    /// 是否启用 Codex 配额预警通知
    pub codex_quota_alert_enabled: bool,
    /// Codex 配额预警阈值（百分比）
    pub codex_quota_alert_threshold: i32,
    /// 是否启用 GitHub Copilot 配额预警通知
    pub ghcp_quota_alert_enabled: bool,
    /// GitHub Copilot 配额预警阈值（百分比）
    pub ghcp_quota_alert_threshold: i32,
    /// 是否启用 Windsurf 配额预警通知
    pub windsurf_quota_alert_enabled: bool,
    /// Windsurf 配额预警阈值（百分比）
    pub windsurf_quota_alert_threshold: i32,
    /// 是否启用 Kiro 配额预警通知
    pub kiro_quota_alert_enabled: bool,
    /// Kiro 配额预警阈值（百分比）
    pub kiro_quota_alert_threshold: i32,
    /// 是否启用 Cursor 配额预警通知
    pub cursor_quota_alert_enabled: bool,
    /// Cursor 配额预警阈值（百分比）
    pub cursor_quota_alert_threshold: i32,
    /// 是否启用 Gemini 配额预警通知
    pub gemini_quota_alert_enabled: bool,
    /// Gemini 配额预警阈值（百分比）
    pub gemini_quota_alert_threshold: i32,
    /// 是否启用 CodeBuddy 配额预警通知
    pub codebuddy_quota_alert_enabled: bool,
    /// CodeBuddy 配额预警阈值（百分比）
    pub codebuddy_quota_alert_threshold: i32,
    /// 是否启用 CodeBuddy CN 配额预警通知
    pub codebuddy_cn_quota_alert_enabled: bool,
    /// CodeBuddy CN 配额预警阈值（百分比）
    pub codebuddy_cn_quota_alert_threshold: i32,
    /// 是否启用 Qoder 配额预警通知
    pub qoder_quota_alert_enabled: bool,
    /// Qoder 配额预警阈值（百分比）
    pub qoder_quota_alert_threshold: i32,
    /// 是否启用 Trae 配额预警通知
    pub trae_quota_alert_enabled: bool,
    /// Trae 配额预警阈值（百分比）
    pub trae_quota_alert_threshold: i32,
}

const DEFAULT_UI_SCALE: f64 = 1.0;
const MIN_UI_SCALE: f64 = 0.8;
const MAX_UI_SCALE: f64 = 2.0;

fn sanitize_ui_scale(raw: f64) -> f64 {
    if !raw.is_finite() {
        return DEFAULT_UI_SCALE;
    }
    raw.clamp(MIN_UI_SCALE, MAX_UI_SCALE)
}

#[tauri::command]
pub async fn open_data_folder() -> Result<(), String> {
    let path = modules::account::get_data_dir()?;

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("打开文件夹失败: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| format!("打开文件夹失败: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("打开文件夹失败: {}", e))?;
    }

    Ok(())
}

/// 保存文本文件
#[tauri::command]
pub async fn save_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("写入文件失败: {}", e))
}

/// 获取下载目录
#[tauri::command]
pub fn get_downloads_dir() -> Result<String, String> {
    if let Some(dir) = dirs::download_dir() {
        return Ok(dir.to_string_lossy().to_string());
    }
    if let Some(home) = dirs::home_dir() {
        return Ok(home.join("Downloads").to_string_lossy().to_string());
    }
    Err("无法获取下载目录".to_string())
}

/// 获取网络服务配置
#[tauri::command]
pub fn get_network_config() -> Result<NetworkConfig, String> {
    let user_config = config::get_user_config();
    let actual_port = config::get_actual_port();

    Ok(NetworkConfig {
        ws_enabled: user_config.ws_enabled,
        ws_port: user_config.ws_port,
        actual_port,
        default_port: DEFAULT_WS_PORT,
    })
}

/// 保存网络服务配置
#[tauri::command]
pub fn save_network_config(ws_enabled: bool, ws_port: u16) -> Result<bool, String> {
    let current = config::get_user_config();
    let needs_restart = current.ws_port != ws_port || current.ws_enabled != ws_enabled;

    let new_config = UserConfig {
        ws_enabled,
        ws_port,
        // 保留其他设置不变
        language: current.language,
        theme: current.theme,
        ui_scale: current.ui_scale,
        auto_refresh_minutes: current.auto_refresh_minutes,
        codex_auto_refresh_minutes: current.codex_auto_refresh_minutes,
        ghcp_auto_refresh_minutes: current.ghcp_auto_refresh_minutes,
        windsurf_auto_refresh_minutes: current.windsurf_auto_refresh_minutes,
        kiro_auto_refresh_minutes: current.kiro_auto_refresh_minutes,
        cursor_auto_refresh_minutes: current.cursor_auto_refresh_minutes,
        gemini_auto_refresh_minutes: current.gemini_auto_refresh_minutes,
        codebuddy_auto_refresh_minutes: current.codebuddy_auto_refresh_minutes,
        codebuddy_cn_auto_refresh_minutes: current.codebuddy_cn_auto_refresh_minutes,
        qoder_auto_refresh_minutes: current.qoder_auto_refresh_minutes,
        trae_auto_refresh_minutes: current.trae_auto_refresh_minutes,
        close_behavior: current.close_behavior,
        minimize_behavior: current.minimize_behavior,
        hide_dock_icon: current.hide_dock_icon,
        opencode_app_path: current.opencode_app_path,
        antigravity_app_path: current.antigravity_app_path,
        codex_app_path: current.codex_app_path,
        vscode_app_path: current.vscode_app_path,
        windsurf_app_path: current.windsurf_app_path,
        kiro_app_path: current.kiro_app_path,
        cursor_app_path: current.cursor_app_path,
        codebuddy_app_path: current.codebuddy_app_path,
        codebuddy_cn_app_path: current.codebuddy_cn_app_path,
        qoder_app_path: current.qoder_app_path,
        trae_app_path: current.trae_app_path,
        opencode_sync_on_switch: current.opencode_sync_on_switch,
        opencode_auth_overwrite_on_switch: current.opencode_auth_overwrite_on_switch,
        codex_launch_on_switch: current.codex_launch_on_switch,
        auto_switch_enabled: current.auto_switch_enabled,
        auto_switch_threshold: current.auto_switch_threshold,
        quota_alert_enabled: current.quota_alert_enabled,
        quota_alert_threshold: current.quota_alert_threshold,
        codex_quota_alert_enabled: current.codex_quota_alert_enabled,
        codex_quota_alert_threshold: current.codex_quota_alert_threshold,
        ghcp_quota_alert_enabled: current.ghcp_quota_alert_enabled,
        ghcp_quota_alert_threshold: current.ghcp_quota_alert_threshold,
        windsurf_quota_alert_enabled: current.windsurf_quota_alert_enabled,
        windsurf_quota_alert_threshold: current.windsurf_quota_alert_threshold,
        kiro_quota_alert_enabled: current.kiro_quota_alert_enabled,
        kiro_quota_alert_threshold: current.kiro_quota_alert_threshold,
        cursor_quota_alert_enabled: current.cursor_quota_alert_enabled,
        cursor_quota_alert_threshold: current.cursor_quota_alert_threshold,
        gemini_quota_alert_enabled: current.gemini_quota_alert_enabled,
        gemini_quota_alert_threshold: current.gemini_quota_alert_threshold,
        codebuddy_quota_alert_enabled: current.codebuddy_quota_alert_enabled,
        codebuddy_quota_alert_threshold: current.codebuddy_quota_alert_threshold,
        codebuddy_cn_quota_alert_enabled: current.codebuddy_cn_quota_alert_enabled,
        codebuddy_cn_quota_alert_threshold: current.codebuddy_cn_quota_alert_threshold,
        qoder_quota_alert_enabled: current.qoder_quota_alert_enabled,
        qoder_quota_alert_threshold: current.qoder_quota_alert_threshold,
        trae_quota_alert_enabled: current.trae_quota_alert_enabled,
        trae_quota_alert_threshold: current.trae_quota_alert_threshold,
    };

    config::save_user_config(&new_config)?;

    Ok(needs_restart)
}

/// 获取通用设置配置
#[tauri::command]
pub fn get_general_config() -> Result<GeneralConfig, String> {
    let user_config = config::get_user_config();

    let close_behavior_str = match user_config.close_behavior {
        CloseWindowBehavior::Ask => "ask",
        CloseWindowBehavior::Minimize => "minimize",
        CloseWindowBehavior::Quit => "quit",
    };
    let minimize_behavior_str = match user_config.minimize_behavior {
        MinimizeWindowBehavior::DockAndTray => "dock_and_tray",
        MinimizeWindowBehavior::TrayOnly => "tray_only",
    };

    Ok(GeneralConfig {
        language: user_config.language,
        theme: user_config.theme,
        ui_scale: user_config.ui_scale,
        auto_refresh_minutes: user_config.auto_refresh_minutes,
        codex_auto_refresh_minutes: user_config.codex_auto_refresh_minutes,
        ghcp_auto_refresh_minutes: user_config.ghcp_auto_refresh_minutes,
        windsurf_auto_refresh_minutes: user_config.windsurf_auto_refresh_minutes,
        kiro_auto_refresh_minutes: user_config.kiro_auto_refresh_minutes,
        cursor_auto_refresh_minutes: user_config.cursor_auto_refresh_minutes,
        gemini_auto_refresh_minutes: user_config.gemini_auto_refresh_minutes,
        codebuddy_auto_refresh_minutes: user_config.codebuddy_auto_refresh_minutes,
        codebuddy_cn_auto_refresh_minutes: user_config.codebuddy_cn_auto_refresh_minutes,
        qoder_auto_refresh_minutes: user_config.qoder_auto_refresh_minutes,
        trae_auto_refresh_minutes: user_config.trae_auto_refresh_minutes,
        close_behavior: close_behavior_str.to_string(),
        minimize_behavior: minimize_behavior_str.to_string(),
        hide_dock_icon: user_config.hide_dock_icon,
        opencode_app_path: user_config.opencode_app_path,
        antigravity_app_path: user_config.antigravity_app_path,
        codex_app_path: user_config.codex_app_path,
        vscode_app_path: user_config.vscode_app_path,
        windsurf_app_path: user_config.windsurf_app_path,
        kiro_app_path: user_config.kiro_app_path,
        cursor_app_path: user_config.cursor_app_path,
        codebuddy_app_path: user_config.codebuddy_app_path,
        codebuddy_cn_app_path: user_config.codebuddy_cn_app_path,
        qoder_app_path: user_config.qoder_app_path,
        trae_app_path: user_config.trae_app_path,
        opencode_sync_on_switch: user_config.opencode_sync_on_switch,
        opencode_auth_overwrite_on_switch: user_config.opencode_auth_overwrite_on_switch,
        codex_launch_on_switch: user_config.codex_launch_on_switch,
        auto_switch_enabled: user_config.auto_switch_enabled,
        auto_switch_threshold: user_config.auto_switch_threshold,
        quota_alert_enabled: user_config.quota_alert_enabled,
        quota_alert_threshold: user_config.quota_alert_threshold,
        codex_quota_alert_enabled: user_config.codex_quota_alert_enabled,
        codex_quota_alert_threshold: user_config.codex_quota_alert_threshold,
        ghcp_quota_alert_enabled: user_config.ghcp_quota_alert_enabled,
        ghcp_quota_alert_threshold: user_config.ghcp_quota_alert_threshold,
        windsurf_quota_alert_enabled: user_config.windsurf_quota_alert_enabled,
        windsurf_quota_alert_threshold: user_config.windsurf_quota_alert_threshold,
        kiro_quota_alert_enabled: user_config.kiro_quota_alert_enabled,
        kiro_quota_alert_threshold: user_config.kiro_quota_alert_threshold,
        cursor_quota_alert_enabled: user_config.cursor_quota_alert_enabled,
        cursor_quota_alert_threshold: user_config.cursor_quota_alert_threshold,
        gemini_quota_alert_enabled: user_config.gemini_quota_alert_enabled,
        gemini_quota_alert_threshold: user_config.gemini_quota_alert_threshold,
        codebuddy_quota_alert_enabled: user_config.codebuddy_quota_alert_enabled,
        codebuddy_quota_alert_threshold: user_config.codebuddy_quota_alert_threshold,
        codebuddy_cn_quota_alert_enabled: user_config.codebuddy_cn_quota_alert_enabled,
        codebuddy_cn_quota_alert_threshold: user_config.codebuddy_cn_quota_alert_threshold,
        qoder_quota_alert_enabled: user_config.qoder_quota_alert_enabled,
        qoder_quota_alert_threshold: user_config.qoder_quota_alert_threshold,
        trae_quota_alert_enabled: user_config.trae_quota_alert_enabled,
        trae_quota_alert_threshold: user_config.trae_quota_alert_threshold,
    })
}

/// 保存通用设置配置
#[tauri::command]
pub fn save_general_config(
    app: tauri::AppHandle,
    language: String,
    theme: String,
    ui_scale: Option<f64>,
    auto_refresh_minutes: i32,
    codex_auto_refresh_minutes: i32,
    ghcp_auto_refresh_minutes: Option<i32>,
    windsurf_auto_refresh_minutes: Option<i32>,
    kiro_auto_refresh_minutes: Option<i32>,
    cursor_auto_refresh_minutes: Option<i32>,
    gemini_auto_refresh_minutes: Option<i32>,
    codebuddy_auto_refresh_minutes: Option<i32>,
    codebuddy_cn_auto_refresh_minutes: Option<i32>,
    qoder_auto_refresh_minutes: Option<i32>,
    trae_auto_refresh_minutes: Option<i32>,
    close_behavior: String,
    minimize_behavior: Option<String>,
    hide_dock_icon: Option<bool>,
    opencode_app_path: String,
    antigravity_app_path: String,
    codex_app_path: String,
    vscode_app_path: String,
    windsurf_app_path: Option<String>,
    kiro_app_path: Option<String>,
    cursor_app_path: Option<String>,
    codebuddy_app_path: Option<String>,
    codebuddy_cn_app_path: Option<String>,
    qoder_app_path: Option<String>,
    trae_app_path: Option<String>,
    opencode_sync_on_switch: bool,
    opencode_auth_overwrite_on_switch: Option<bool>,
    codex_launch_on_switch: bool,
    auto_switch_enabled: Option<bool>,
    auto_switch_threshold: Option<i32>,
    quota_alert_enabled: Option<bool>,
    quota_alert_threshold: Option<i32>,
    codex_quota_alert_enabled: Option<bool>,
    codex_quota_alert_threshold: Option<i32>,
    ghcp_quota_alert_enabled: Option<bool>,
    ghcp_quota_alert_threshold: Option<i32>,
    windsurf_quota_alert_enabled: Option<bool>,
    windsurf_quota_alert_threshold: Option<i32>,
    kiro_quota_alert_enabled: Option<bool>,
    kiro_quota_alert_threshold: Option<i32>,
    cursor_quota_alert_enabled: Option<bool>,
    cursor_quota_alert_threshold: Option<i32>,
    gemini_quota_alert_enabled: Option<bool>,
    gemini_quota_alert_threshold: Option<i32>,
    codebuddy_quota_alert_enabled: Option<bool>,
    codebuddy_quota_alert_threshold: Option<i32>,
    codebuddy_cn_quota_alert_enabled: Option<bool>,
    codebuddy_cn_quota_alert_threshold: Option<i32>,
    qoder_quota_alert_enabled: Option<bool>,
    qoder_quota_alert_threshold: Option<i32>,
    trae_quota_alert_enabled: Option<bool>,
    trae_quota_alert_threshold: Option<i32>,
) -> Result<(), String> {
    let current = config::get_user_config();
    let normalized_opencode_path = opencode_app_path.trim().to_string();
    let normalized_antigravity_path = antigravity_app_path.trim().to_string();
    let normalized_codex_path = codex_app_path.trim().to_string();
    let normalized_vscode_path = vscode_app_path.trim().to_string();
    let normalized_ui_scale = sanitize_ui_scale(ui_scale.unwrap_or(current.ui_scale));
    let normalized_windsurf_path = windsurf_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.windsurf_app_path.clone());
    let normalized_kiro_path = kiro_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.kiro_app_path.clone());
    let normalized_cursor_path = cursor_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.cursor_app_path.clone());
    let normalized_codebuddy_path = codebuddy_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.codebuddy_app_path.clone());
    let normalized_codebuddy_cn_path = codebuddy_cn_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.codebuddy_cn_app_path.clone());
    let normalized_qoder_path = qoder_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.qoder_app_path.clone());
    let normalized_trae_path = trae_app_path
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| current.trae_app_path.clone());
    // 标准化语言代码为小写，确保与插件端格式一致
    let normalized_language = language.to_lowercase();
    let language_changed = current.language != normalized_language;
    let language_for_broadcast = normalized_language.clone();

    // 解析关闭行为
    let close_behavior_enum = match close_behavior.as_str() {
        "minimize" => CloseWindowBehavior::Minimize,
        "quit" => CloseWindowBehavior::Quit,
        _ => CloseWindowBehavior::Ask,
    };
    let minimize_behavior_enum = match minimize_behavior.as_deref() {
        Some("dock_and_tray") => MinimizeWindowBehavior::DockAndTray,
        Some("tray_only") => MinimizeWindowBehavior::TrayOnly,
        Some(_) | None => current.minimize_behavior.clone(),
    };
    let hide_dock_icon_value = hide_dock_icon.unwrap_or(current.hide_dock_icon);
    #[cfg(target_os = "macos")]
    let hide_dock_icon_changed = current.hide_dock_icon != hide_dock_icon_value;

    let new_config = UserConfig {
        // 保留网络设置不变
        ws_enabled: current.ws_enabled,
        ws_port: current.ws_port,
        // 更新通用设置
        language: normalized_language.clone(),
        theme,
        ui_scale: normalized_ui_scale,
        auto_refresh_minutes,
        codex_auto_refresh_minutes,
        ghcp_auto_refresh_minutes: ghcp_auto_refresh_minutes
            .unwrap_or(current.ghcp_auto_refresh_minutes),
        windsurf_auto_refresh_minutes: windsurf_auto_refresh_minutes
            .unwrap_or(current.windsurf_auto_refresh_minutes),
        kiro_auto_refresh_minutes: kiro_auto_refresh_minutes
            .unwrap_or(current.kiro_auto_refresh_minutes),
        cursor_auto_refresh_minutes: cursor_auto_refresh_minutes
            .unwrap_or(current.cursor_auto_refresh_minutes),
        gemini_auto_refresh_minutes: gemini_auto_refresh_minutes
            .unwrap_or(current.gemini_auto_refresh_minutes),
        codebuddy_auto_refresh_minutes: codebuddy_auto_refresh_minutes
            .unwrap_or(current.codebuddy_auto_refresh_minutes),
        codebuddy_cn_auto_refresh_minutes: codebuddy_cn_auto_refresh_minutes
            .unwrap_or(current.codebuddy_cn_auto_refresh_minutes),
        qoder_auto_refresh_minutes: qoder_auto_refresh_minutes
            .unwrap_or(current.qoder_auto_refresh_minutes),
        trae_auto_refresh_minutes: trae_auto_refresh_minutes
            .unwrap_or(current.trae_auto_refresh_minutes),
        close_behavior: close_behavior_enum,
        minimize_behavior: minimize_behavior_enum,
        hide_dock_icon: hide_dock_icon_value,
        opencode_app_path: normalized_opencode_path,
        antigravity_app_path: normalized_antigravity_path,
        codex_app_path: normalized_codex_path,
        vscode_app_path: normalized_vscode_path,
        windsurf_app_path: normalized_windsurf_path,
        kiro_app_path: normalized_kiro_path,
        cursor_app_path: normalized_cursor_path,
        codebuddy_app_path: normalized_codebuddy_path,
        codebuddy_cn_app_path: normalized_codebuddy_cn_path,
        qoder_app_path: normalized_qoder_path,
        trae_app_path: normalized_trae_path,
        opencode_sync_on_switch,
        opencode_auth_overwrite_on_switch: opencode_auth_overwrite_on_switch
            .unwrap_or(current.opencode_auth_overwrite_on_switch),
        codex_launch_on_switch,
        auto_switch_enabled: auto_switch_enabled.unwrap_or(current.auto_switch_enabled),
        auto_switch_threshold: auto_switch_threshold.unwrap_or(current.auto_switch_threshold),
        quota_alert_enabled: quota_alert_enabled.unwrap_or(current.quota_alert_enabled),
        quota_alert_threshold: quota_alert_threshold.unwrap_or(current.quota_alert_threshold),
        codex_quota_alert_enabled: codex_quota_alert_enabled
            .unwrap_or(current.codex_quota_alert_enabled),
        codex_quota_alert_threshold: codex_quota_alert_threshold
            .unwrap_or(current.codex_quota_alert_threshold),
        ghcp_quota_alert_enabled: ghcp_quota_alert_enabled
            .unwrap_or(current.ghcp_quota_alert_enabled),
        ghcp_quota_alert_threshold: ghcp_quota_alert_threshold
            .unwrap_or(current.ghcp_quota_alert_threshold),
        windsurf_quota_alert_enabled: windsurf_quota_alert_enabled
            .unwrap_or(current.windsurf_quota_alert_enabled),
        windsurf_quota_alert_threshold: windsurf_quota_alert_threshold
            .unwrap_or(current.windsurf_quota_alert_threshold),
        kiro_quota_alert_enabled: kiro_quota_alert_enabled
            .unwrap_or(current.kiro_quota_alert_enabled),
        kiro_quota_alert_threshold: kiro_quota_alert_threshold
            .unwrap_or(current.kiro_quota_alert_threshold),
        cursor_quota_alert_enabled: cursor_quota_alert_enabled
            .unwrap_or(current.cursor_quota_alert_enabled),
        cursor_quota_alert_threshold: cursor_quota_alert_threshold
            .unwrap_or(current.cursor_quota_alert_threshold),
        gemini_quota_alert_enabled: gemini_quota_alert_enabled
            .unwrap_or(current.gemini_quota_alert_enabled),
        gemini_quota_alert_threshold: gemini_quota_alert_threshold
            .unwrap_or(current.gemini_quota_alert_threshold),
        codebuddy_quota_alert_enabled: codebuddy_quota_alert_enabled
            .unwrap_or(current.codebuddy_quota_alert_enabled),
        codebuddy_quota_alert_threshold: codebuddy_quota_alert_threshold
            .unwrap_or(current.codebuddy_quota_alert_threshold),
        codebuddy_cn_quota_alert_enabled: codebuddy_cn_quota_alert_enabled
            .unwrap_or(current.codebuddy_cn_quota_alert_enabled),
        codebuddy_cn_quota_alert_threshold: codebuddy_cn_quota_alert_threshold
            .unwrap_or(current.codebuddy_cn_quota_alert_threshold),
        qoder_quota_alert_enabled: qoder_quota_alert_enabled
            .unwrap_or(current.qoder_quota_alert_enabled),
        qoder_quota_alert_threshold: qoder_quota_alert_threshold
            .unwrap_or(current.qoder_quota_alert_threshold),
        trae_quota_alert_enabled: trae_quota_alert_enabled
            .unwrap_or(current.trae_quota_alert_enabled),
        trae_quota_alert_threshold: trae_quota_alert_threshold
            .unwrap_or(current.trae_quota_alert_threshold),
    };

    config::save_user_config(&new_config)?;

    #[cfg(target_os = "macos")]
    if hide_dock_icon_changed {
        crate::apply_macos_activation_policy(&app);
    }

    if language_changed {
        // 广播语言变更（如果有客户端连接，会通过 WebSocket 发送）
        websocket::broadcast_language_changed(&language_for_broadcast, "desktop");

        // 同时写入共享文件（供插件端离线时启动读取）
        // 因为无法确定插件端是否收到了 WebSocket 消息，保守策略是总是写入
        // 但为了减少写入，可以检查是否有客户端连接
        // 这里简化处理：总是写入，插件端启动时会比较时间戳
        modules::sync_settings::write_sync_setting("language", &normalized_language);

        // 仅在语言变更时刷新托盘菜单，避免无关配置触发托盘重建
        if let Err(err) = modules::tray::update_tray_menu(&app) {
            modules::logger::log_warn(&format!("[Tray] 语言变更后刷新托盘失败: {}", err));
        }
    }

    Ok(())
}

#[tauri::command]
pub fn save_tray_platform_layout(
    app: tauri::AppHandle,
    sort_mode: String,
    ordered_platform_ids: Vec<String>,
    tray_platform_ids: Vec<String>,
) -> Result<(), String> {
    modules::tray_layout::save_tray_layout(sort_mode, ordered_platform_ids, tray_platform_ids)?;
    modules::tray::update_tray_menu(&app)?;
    Ok(())
}

#[tauri::command]
pub fn set_app_path(app: String, path: String) -> Result<(), String> {
    let mut current = config::get_user_config();
    let normalized_path = path.trim().to_string();
    match app.as_str() {
        "antigravity" => current.antigravity_app_path = normalized_path,
        "codex" => current.codex_app_path = normalized_path,
        "vscode" => current.vscode_app_path = normalized_path,
        "windsurf" => current.windsurf_app_path = normalized_path,
        "kiro" => current.kiro_app_path = normalized_path,
        "cursor" => current.cursor_app_path = normalized_path,
        "codebuddy" => current.codebuddy_app_path = normalized_path,
        "codebuddy_cn" => current.codebuddy_cn_app_path = normalized_path,
        "qoder" => current.qoder_app_path = normalized_path,
        "trae" => current.trae_app_path = normalized_path,
        "opencode" => current.opencode_app_path = normalized_path,
        _ => return Err("未知应用类型".to_string()),
    }
    config::save_user_config(&current)?;
    Ok(())
}

#[tauri::command]
pub fn detect_app_path(app: String, force: Option<bool>) -> Result<Option<String>, String> {
    let force = force.unwrap_or(false);
    match app.as_str() {
        "windsurf" => Ok(modules::windsurf_instance::detect_and_save_windsurf_launch_path(force)),
        "kiro" => Ok(modules::kiro_instance::detect_and_save_kiro_launch_path(
            force,
        )),
        "cursor" => Ok(modules::cursor_instance::detect_and_save_cursor_launch_path(force)),
        "antigravity" | "codex" | "vscode" | "codebuddy" | "codebuddy_cn" | "qoder" | "trae"
        | "opencode" => Ok(modules::process::detect_and_save_app_path(
            app.as_str(),
            force,
        )),
        _ => Err("未知应用类型".to_string()),
    }
}

/// 通知插件关闭/开启唤醒功能（互斥）
#[tauri::command]
pub fn set_wakeup_override(enabled: bool) -> Result<(), String> {
    websocket::broadcast_wakeup_override(enabled);
    Ok(())
}

/// 执行窗口关闭操作
/// action: "minimize" | "quit"
/// remember: 是否记住选择
#[tauri::command]
pub fn handle_window_close(
    window: tauri::Window,
    action: String,
    remember: bool,
) -> Result<(), String> {
    modules::logger::log_info(&format!(
        "[Window] 用户选择: action={}, remember={}",
        action, remember
    ));

    // 如果需要记住选择，更新配置
    if remember {
        let current = config::get_user_config();
        let close_behavior = match action.as_str() {
            "minimize" => CloseWindowBehavior::Minimize,
            "quit" => CloseWindowBehavior::Quit,
            _ => CloseWindowBehavior::Ask,
        };

        let new_config = UserConfig {
            close_behavior,
            ..current
        };

        config::save_user_config(&new_config)?;
        modules::logger::log_info(&format!("[Window] 已保存关闭行为设置: {}", action));
    }

    // 执行操作
    match action.as_str() {
        "minimize" => {
            let _ = window.hide();
            modules::logger::log_info("[Window] 窗口已最小化到托盘");
        }
        "quit" => {
            window.app_handle().exit(0);
        }
        _ => {
            return Err("无效的操作".to_string());
        }
    }

    Ok(())
}

/// 打开指定文件夹（如不存在则创建）
#[tauri::command]
pub async fn open_folder(path: String) -> Result<(), String> {
    let folder_path = std::path::Path::new(&path);

    // 如果目录不存在则创建
    if !folder_path.exists() {
        std::fs::create_dir_all(folder_path).map_err(|e| format!("创建文件夹失败: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("打开文件夹失败: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("打开文件夹失败: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("打开文件夹失败: {}", e))?;
    }

    Ok(())
}

/// 删除损坏的文件（会先备份）
#[tauri::command]
pub async fn delete_corrupted_file(path: String) -> Result<(), String> {
    let file_path = std::path::Path::new(&path);

    if !file_path.exists() {
        // 文件不存在，直接返回成功
        return Ok(());
    }

    // 创建备份文件名
    let timestamp = chrono::Utc::now().timestamp();
    let backup_name = format!("{}.corrupted.{}", path, timestamp);

    // 备份文件
    std::fs::rename(&path, &backup_name).map_err(|e| format!("备份损坏文件失败: {}", e))?;

    modules::logger::log_info(&format!(
        "已备份并删除损坏文件: {} -> {}",
        path, backup_name
    ));

    Ok(())
}
