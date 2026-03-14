//! 配置服务模块
//! 管理应用配置，包括 WebSocket 端口等

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

/// 默认 WebSocket 端口
pub const DEFAULT_WS_PORT: u16 = 19528;

/// 端口尝试范围（从配置端口开始，最多尝试 100 个）
pub const PORT_RANGE: u16 = 100;

/// 服务状态配置文件名（供外部客户端读取）
const SERVER_STATUS_FILE: &str = "server.json";

/// 用户配置文件名
const USER_CONFIG_FILE: &str = "config.json";

/// 数据目录名
const DATA_DIR: &str = ".antigravity_cockpit";

/// 服务状态（写入共享文件供其他客户端读取）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    /// WebSocket 服务端口（实际绑定的端口）
    pub ws_port: u16,
    /// 服务版本
    pub version: String,
    /// 进程 ID（用于检测服务是否存活）
    pub pid: u32,
    /// 启动时间戳
    pub started_at: i64,
}

/// 用户配置（持久化存储）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    /// WebSocket 服务是否启用
    #[serde(default = "default_ws_enabled")]
    pub ws_enabled: bool,
    /// WebSocket 首选端口（用户配置的，实际可能不同）
    #[serde(default = "default_ws_port")]
    pub ws_port: u16,
    /// 界面语言
    #[serde(default = "default_language")]
    pub language: String,
    /// 应用主题
    #[serde(default = "default_theme")]
    pub theme: String,
    /// 界面缩放比例
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f64,
    /// 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_auto_refresh")]
    pub auto_refresh_minutes: i32,
    /// Codex 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_codex_auto_refresh")]
    pub codex_auto_refresh_minutes: i32,
    /// GitHub Copilot 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_ghcp_auto_refresh")]
    pub ghcp_auto_refresh_minutes: i32,
    /// Windsurf 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_windsurf_auto_refresh")]
    pub windsurf_auto_refresh_minutes: i32,
    /// Kiro 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_kiro_auto_refresh")]
    pub kiro_auto_refresh_minutes: i32,
    /// Cursor 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_cursor_auto_refresh")]
    pub cursor_auto_refresh_minutes: i32,
    /// Gemini 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_gemini_auto_refresh")]
    pub gemini_auto_refresh_minutes: i32,
    /// CodeBuddy 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_codebuddy_auto_refresh")]
    pub codebuddy_auto_refresh_minutes: i32,
    /// CodeBuddy CN 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_codebuddy_cn_auto_refresh")]
    pub codebuddy_cn_auto_refresh_minutes: i32,
    /// Qoder 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_qoder_auto_refresh")]
    pub qoder_auto_refresh_minutes: i32,
    /// Trae 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_trae_auto_refresh")]
    pub trae_auto_refresh_minutes: i32,
    /// 窗口关闭行为
    #[serde(default = "default_close_behavior")]
    pub close_behavior: CloseWindowBehavior,
    /// 窗口最小化行为（macOS）
    #[serde(default = "default_minimize_behavior")]
    pub minimize_behavior: MinimizeWindowBehavior,
    /// 是否隐藏 Dock 图标（macOS）
    #[serde(default = "default_hide_dock_icon")]
    pub hide_dock_icon: bool,
    /// OpenCode 启动路径（为空则使用默认路径）
    #[serde(default = "default_opencode_app_path")]
    pub opencode_app_path: String,
    /// Antigravity 启动路径（为空则使用默认路径）
    #[serde(default = "default_antigravity_app_path")]
    pub antigravity_app_path: String,
    /// Codex 启动路径（为空则使用默认路径）
    #[serde(default = "default_codex_app_path")]
    pub codex_app_path: String,
    /// VS Code 启动路径（为空则使用默认路径）
    #[serde(default = "default_vscode_app_path")]
    pub vscode_app_path: String,
    /// Windsurf 启动路径（为空则使用默认路径）
    #[serde(default = "default_windsurf_app_path")]
    pub windsurf_app_path: String,
    /// Kiro 启动路径（为空则使用默认路径）
    #[serde(default = "default_kiro_app_path")]
    pub kiro_app_path: String,
    /// Cursor 启动路径（为空则使用默认路径）
    #[serde(default = "default_cursor_app_path")]
    pub cursor_app_path: String,
    /// CodeBuddy 启动路径（为空则使用默认路径）
    #[serde(default = "default_codebuddy_app_path")]
    pub codebuddy_app_path: String,
    /// CodeBuddy CN 启动路径（为空则使用默认路径）
    #[serde(default = "default_codebuddy_cn_app_path")]
    pub codebuddy_cn_app_path: String,
    /// Qoder 启动路径（为空则使用默认路径）
    #[serde(default = "default_qoder_app_path")]
    pub qoder_app_path: String,
    /// Trae 启动路径（为空则使用默认路径）
    #[serde(default = "default_trae_app_path")]
    pub trae_app_path: String,
    /// 切换 Codex 时是否自动重启 OpenCode
    #[serde(default = "default_opencode_sync_on_switch")]
    pub opencode_sync_on_switch: bool,
    /// 切换 Codex 时是否覆盖 OpenCode 登录信息
    #[serde(default = "default_opencode_auth_overwrite_on_switch")]
    pub opencode_auth_overwrite_on_switch: bool,
    /// 切换 Codex 时是否自动启动/重启 Codex App
    #[serde(default = "default_codex_launch_on_switch")]
    pub codex_launch_on_switch: bool,
    /// 是否启用自动切号
    #[serde(default = "default_auto_switch_enabled")]
    pub auto_switch_enabled: bool,
    /// 自动切号阈值（百分比），任意模型配额低于此值触发
    #[serde(default = "default_auto_switch_threshold")]
    pub auto_switch_threshold: i32,
    /// 是否启用配额预警通知
    #[serde(default = "default_quota_alert_enabled")]
    pub quota_alert_enabled: bool,
    /// 配额预警阈值（百分比），任意模型配额低于此值触发
    #[serde(default = "default_quota_alert_threshold")]
    pub quota_alert_threshold: i32,
    /// 是否启用 Codex 配额预警通知
    #[serde(default = "default_codex_quota_alert_enabled")]
    pub codex_quota_alert_enabled: bool,
    /// Codex 配额预警阈值（百分比）
    #[serde(default = "default_codex_quota_alert_threshold")]
    pub codex_quota_alert_threshold: i32,
    /// 是否启用 GitHub Copilot 配额预警通知
    #[serde(default = "default_ghcp_quota_alert_enabled")]
    pub ghcp_quota_alert_enabled: bool,
    /// GitHub Copilot 配额预警阈值（百分比）
    #[serde(default = "default_ghcp_quota_alert_threshold")]
    pub ghcp_quota_alert_threshold: i32,
    /// 是否启用 Windsurf 配额预警通知
    #[serde(default = "default_windsurf_quota_alert_enabled")]
    pub windsurf_quota_alert_enabled: bool,
    /// Windsurf 配额预警阈值（百分比）
    #[serde(default = "default_windsurf_quota_alert_threshold")]
    pub windsurf_quota_alert_threshold: i32,
    /// 是否启用 Kiro 配额预警通知
    #[serde(default = "default_kiro_quota_alert_enabled")]
    pub kiro_quota_alert_enabled: bool,
    /// Kiro 配额预警阈值（百分比）
    #[serde(default = "default_kiro_quota_alert_threshold")]
    pub kiro_quota_alert_threshold: i32,
    /// 是否启用 Cursor 配额预警通知
    #[serde(default = "default_cursor_quota_alert_enabled")]
    pub cursor_quota_alert_enabled: bool,
    /// Cursor 配额预警阈值（百分比）
    #[serde(default = "default_cursor_quota_alert_threshold")]
    pub cursor_quota_alert_threshold: i32,
    /// 是否启用 Gemini 配额预警通知
    #[serde(default = "default_gemini_quota_alert_enabled")]
    pub gemini_quota_alert_enabled: bool,
    /// Gemini 配额预警阈值（百分比）
    #[serde(default = "default_gemini_quota_alert_threshold")]
    pub gemini_quota_alert_threshold: i32,
    /// 是否启用 CodeBuddy 配额预警通知
    #[serde(default = "default_codebuddy_quota_alert_enabled")]
    pub codebuddy_quota_alert_enabled: bool,
    /// CodeBuddy 配额预警阈值（百分比）
    #[serde(default = "default_codebuddy_quota_alert_threshold")]
    pub codebuddy_quota_alert_threshold: i32,
    /// 是否启用 CodeBuddy CN 配额预警通知
    #[serde(default = "default_codebuddy_cn_quota_alert_enabled")]
    pub codebuddy_cn_quota_alert_enabled: bool,
    /// CodeBuddy CN 配额预警阈值（百分比）
    #[serde(default = "default_codebuddy_cn_quota_alert_threshold")]
    pub codebuddy_cn_quota_alert_threshold: i32,
    /// 是否启用 Qoder 配额预警通知
    #[serde(default = "default_qoder_quota_alert_enabled")]
    pub qoder_quota_alert_enabled: bool,
    /// Qoder 配额预警阈值（百分比）
    #[serde(default = "default_qoder_quota_alert_threshold")]
    pub qoder_quota_alert_threshold: i32,
    /// 是否启用 Trae 配额预警通知
    #[serde(default = "default_trae_quota_alert_enabled")]
    pub trae_quota_alert_enabled: bool,
    /// Trae 配额预警阈值（百分比）
    #[serde(default = "default_trae_quota_alert_threshold")]
    pub trae_quota_alert_threshold: i32,
}

/// 窗口关闭行为
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CloseWindowBehavior {
    /// 每次询问
    Ask,
    /// 最小化到托盘
    Minimize,
    /// 退出应用
    Quit,
}

impl Default for CloseWindowBehavior {
    fn default() -> Self {
        CloseWindowBehavior::Ask
    }
}

/// 窗口最小化行为（macOS）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MinimizeWindowBehavior {
    /// 程序坞 + 菜单栏（系统默认最小化）
    DockAndTray,
    /// 仅菜单栏（最小化时隐藏窗口）
    TrayOnly,
}

impl Default for MinimizeWindowBehavior {
    fn default() -> Self {
        MinimizeWindowBehavior::DockAndTray
    }
}

fn default_ws_enabled() -> bool {
    true
}
fn default_ws_port() -> u16 {
    DEFAULT_WS_PORT
}
fn default_language() -> String {
    "zh-cn".to_string()
}
fn default_theme() -> String {
    "system".to_string()
}
fn default_ui_scale() -> f64 {
    1.0
}
fn default_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_codex_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_ghcp_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_windsurf_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_kiro_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_cursor_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_gemini_auto_refresh() -> i32 {
    10
}
fn default_codebuddy_auto_refresh() -> i32 {
    10
}
fn default_codebuddy_cn_auto_refresh() -> i32 {
    10
}
fn default_qoder_auto_refresh() -> i32 {
    10
}
fn default_trae_auto_refresh() -> i32 {
    10
}
fn default_close_behavior() -> CloseWindowBehavior {
    CloseWindowBehavior::Ask
}
fn default_minimize_behavior() -> MinimizeWindowBehavior {
    MinimizeWindowBehavior::DockAndTray
}
fn default_hide_dock_icon() -> bool {
    false
}
fn default_opencode_app_path() -> String {
    String::new()
}
fn default_antigravity_app_path() -> String {
    String::new()
}
fn default_codex_app_path() -> String {
    String::new()
}
fn default_vscode_app_path() -> String {
    String::new()
}
fn default_windsurf_app_path() -> String {
    String::new()
}
fn default_kiro_app_path() -> String {
    String::new()
}
fn default_cursor_app_path() -> String {
    String::new()
}
fn default_codebuddy_app_path() -> String {
    String::new()
}
fn default_codebuddy_cn_app_path() -> String {
    String::new()
}
fn default_qoder_app_path() -> String {
    String::new()
}
fn default_trae_app_path() -> String {
    String::new()
}
fn default_opencode_sync_on_switch() -> bool {
    true
}
fn default_opencode_auth_overwrite_on_switch() -> bool {
    true
}
fn default_codex_launch_on_switch() -> bool {
    true
}
fn default_auto_switch_enabled() -> bool {
    false
}
fn default_auto_switch_threshold() -> i32 {
    5
}
fn default_quota_alert_enabled() -> bool {
    false
}
fn default_quota_alert_threshold() -> i32 {
    20
}
fn default_codex_quota_alert_enabled() -> bool {
    false
}
fn default_codex_quota_alert_threshold() -> i32 {
    20
}
fn default_ghcp_quota_alert_enabled() -> bool {
    false
}
fn default_ghcp_quota_alert_threshold() -> i32 {
    20
}
fn default_windsurf_quota_alert_enabled() -> bool {
    false
}
fn default_windsurf_quota_alert_threshold() -> i32 {
    20
}
fn default_kiro_quota_alert_enabled() -> bool {
    false
}
fn default_kiro_quota_alert_threshold() -> i32 {
    20
}
fn default_cursor_quota_alert_enabled() -> bool {
    false
}
fn default_cursor_quota_alert_threshold() -> i32 {
    20
}
fn default_gemini_quota_alert_enabled() -> bool {
    false
}
fn default_gemini_quota_alert_threshold() -> i32 {
    20
}
fn default_codebuddy_quota_alert_enabled() -> bool {
    false
}
fn default_codebuddy_quota_alert_threshold() -> i32 {
    20
}
fn default_codebuddy_cn_quota_alert_enabled() -> bool {
    false
}
fn default_codebuddy_cn_quota_alert_threshold() -> i32 {
    20
}
fn default_qoder_quota_alert_enabled() -> bool {
    false
}
fn default_qoder_quota_alert_threshold() -> i32 {
    20
}
fn default_trae_quota_alert_enabled() -> bool {
    false
}
fn default_trae_quota_alert_threshold() -> i32 {
    20
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            ws_enabled: true,
            ws_port: DEFAULT_WS_PORT,
            language: default_language(),
            theme: default_theme(),
            ui_scale: default_ui_scale(),
            auto_refresh_minutes: default_auto_refresh(),
            codex_auto_refresh_minutes: default_codex_auto_refresh(),
            ghcp_auto_refresh_minutes: default_ghcp_auto_refresh(),
            windsurf_auto_refresh_minutes: default_windsurf_auto_refresh(),
            kiro_auto_refresh_minutes: default_kiro_auto_refresh(),
            cursor_auto_refresh_minutes: default_cursor_auto_refresh(),
            gemini_auto_refresh_minutes: default_gemini_auto_refresh(),
            codebuddy_auto_refresh_minutes: default_codebuddy_auto_refresh(),
            codebuddy_cn_auto_refresh_minutes: default_codebuddy_cn_auto_refresh(),
            qoder_auto_refresh_minutes: default_qoder_auto_refresh(),
            trae_auto_refresh_minutes: default_trae_auto_refresh(),
            close_behavior: default_close_behavior(),
            minimize_behavior: default_minimize_behavior(),
            hide_dock_icon: default_hide_dock_icon(),
            opencode_app_path: default_opencode_app_path(),
            antigravity_app_path: default_antigravity_app_path(),
            codex_app_path: default_codex_app_path(),
            vscode_app_path: default_vscode_app_path(),
            windsurf_app_path: default_windsurf_app_path(),
            kiro_app_path: default_kiro_app_path(),
            cursor_app_path: default_cursor_app_path(),
            codebuddy_app_path: default_codebuddy_app_path(),
            codebuddy_cn_app_path: default_codebuddy_cn_app_path(),
            qoder_app_path: default_qoder_app_path(),
            trae_app_path: default_trae_app_path(),
            opencode_sync_on_switch: default_opencode_sync_on_switch(),
            opencode_auth_overwrite_on_switch: default_opencode_auth_overwrite_on_switch(),
            codex_launch_on_switch: default_codex_launch_on_switch(),
            auto_switch_enabled: default_auto_switch_enabled(),
            auto_switch_threshold: default_auto_switch_threshold(),
            quota_alert_enabled: default_quota_alert_enabled(),
            quota_alert_threshold: default_quota_alert_threshold(),
            codex_quota_alert_enabled: default_codex_quota_alert_enabled(),
            codex_quota_alert_threshold: default_codex_quota_alert_threshold(),
            ghcp_quota_alert_enabled: default_ghcp_quota_alert_enabled(),
            ghcp_quota_alert_threshold: default_ghcp_quota_alert_threshold(),
            windsurf_quota_alert_enabled: default_windsurf_quota_alert_enabled(),
            windsurf_quota_alert_threshold: default_windsurf_quota_alert_threshold(),
            kiro_quota_alert_enabled: default_kiro_quota_alert_enabled(),
            kiro_quota_alert_threshold: default_kiro_quota_alert_threshold(),
            cursor_quota_alert_enabled: default_cursor_quota_alert_enabled(),
            cursor_quota_alert_threshold: default_cursor_quota_alert_threshold(),
            gemini_quota_alert_enabled: default_gemini_quota_alert_enabled(),
            gemini_quota_alert_threshold: default_gemini_quota_alert_threshold(),
            codebuddy_quota_alert_enabled: default_codebuddy_quota_alert_enabled(),
            codebuddy_quota_alert_threshold: default_codebuddy_quota_alert_threshold(),
            codebuddy_cn_quota_alert_enabled: default_codebuddy_cn_quota_alert_enabled(),
            codebuddy_cn_quota_alert_threshold: default_codebuddy_cn_quota_alert_threshold(),
            qoder_quota_alert_enabled: default_qoder_quota_alert_enabled(),
            qoder_quota_alert_threshold: default_qoder_quota_alert_threshold(),
            trae_quota_alert_enabled: default_trae_quota_alert_enabled(),
            trae_quota_alert_threshold: default_trae_quota_alert_threshold(),
        }
    }
}

/// 运行时状态
struct RuntimeState {
    /// 当前实际使用的端口
    actual_port: Option<u16>,
    /// 用户配置
    user_config: UserConfig,
}

/// 全局运行时状态
static RUNTIME_STATE: OnceLock<RwLock<RuntimeState>> = OnceLock::new();

fn get_runtime_state() -> &'static RwLock<RuntimeState> {
    RUNTIME_STATE.get_or_init(|| {
        RwLock::new(RuntimeState {
            actual_port: None,
            user_config: load_user_config().unwrap_or_default(),
        })
    })
}

/// 获取数据目录路径
pub fn get_data_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("无法获取 Home 目录")?;
    Ok(home.join(DATA_DIR))
}

/// 获取共享目录路径（供其他模块使用）
/// 与 get_data_dir 相同，但不返回 Result
pub fn get_shared_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(DATA_DIR))
        .unwrap_or_else(|| PathBuf::from(DATA_DIR))
}

/// 获取服务状态文件路径
pub fn get_server_status_path() -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    Ok(data_dir.join(SERVER_STATUS_FILE))
}

/// 获取用户配置文件路径
pub fn get_user_config_path() -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    Ok(data_dir.join(USER_CONFIG_FILE))
}

/// 加载用户配置
pub fn load_user_config() -> Result<UserConfig, String> {
    let config_path = get_user_config_path()?;

    if !config_path.exists() {
        return Ok(UserConfig::default());
    }

    let content =
        fs::read_to_string(&config_path).map_err(|e| format!("读取配置文件失败: {}", e))?;

    let mut value: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("解析配置文件失败: {}", e))?;

    // 兼容旧配置：平台独立预警字段不存在时，继承历史全局预警配置
    if let Some(obj) = value.as_object_mut() {
        if !obj.contains_key("kiro_auto_refresh_minutes") {
            let inherited_refresh = obj
                .get("windsurf_auto_refresh_minutes")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(default_kiro_auto_refresh);
            obj.insert(
                "kiro_auto_refresh_minutes".to_string(),
                json!(inherited_refresh),
            );
        }

        if !obj.contains_key("cursor_auto_refresh_minutes") {
            let inherited_refresh = obj
                .get("kiro_auto_refresh_minutes")
                .or_else(|| obj.get("windsurf_auto_refresh_minutes"))
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(default_cursor_auto_refresh);
            obj.insert(
                "cursor_auto_refresh_minutes".to_string(),
                json!(inherited_refresh),
            );
        }

        if !obj.contains_key("gemini_auto_refresh_minutes") {
            let inherited_refresh = obj
                .get("cursor_auto_refresh_minutes")
                .or_else(|| obj.get("kiro_auto_refresh_minutes"))
                .or_else(|| obj.get("windsurf_auto_refresh_minutes"))
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(default_gemini_auto_refresh);
            obj.insert(
                "gemini_auto_refresh_minutes".to_string(),
                json!(inherited_refresh),
            );
        }

        if !obj.contains_key("qoder_auto_refresh_minutes") {
            let inherited_refresh = obj
                .get("gemini_auto_refresh_minutes")
                .or_else(|| obj.get("cursor_auto_refresh_minutes"))
                .or_else(|| obj.get("kiro_auto_refresh_minutes"))
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(default_qoder_auto_refresh);
            obj.insert(
                "qoder_auto_refresh_minutes".to_string(),
                json!(inherited_refresh),
            );
        }

        if !obj.contains_key("codebuddy_cn_auto_refresh_minutes") {
            let inherited_refresh = obj
                .get("codebuddy_auto_refresh_minutes")
                .or_else(|| obj.get("gemini_auto_refresh_minutes"))
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(default_codebuddy_cn_auto_refresh);
            obj.insert(
                "codebuddy_cn_auto_refresh_minutes".to_string(),
                json!(inherited_refresh),
            );
        }

        if !obj.contains_key("trae_auto_refresh_minutes") {
            let inherited_refresh = obj
                .get("qoder_auto_refresh_minutes")
                .or_else(|| obj.get("gemini_auto_refresh_minutes"))
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(default_trae_auto_refresh);
            obj.insert(
                "trae_auto_refresh_minutes".to_string(),
                json!(inherited_refresh),
            );
        }

        if !obj.contains_key("hide_dock_icon") {
            let inherited_hide_dock_icon = obj
                .get("minimize_behavior")
                .and_then(|v| v.as_str())
                .map(|v| v == "tray_only")
                .unwrap_or_else(default_hide_dock_icon);
            obj.insert(
                "hide_dock_icon".to_string(),
                json!(inherited_hide_dock_icon),
            );
        }

        let legacy_enabled = obj
            .get("quota_alert_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(default_quota_alert_enabled);
        let legacy_threshold = obj
            .get("quota_alert_threshold")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or_else(default_quota_alert_threshold);

        if !obj.contains_key("codex_quota_alert_enabled") {
            obj.insert(
                "codex_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("codex_quota_alert_threshold") {
            obj.insert(
                "codex_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("ghcp_quota_alert_enabled") {
            obj.insert(
                "ghcp_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("ghcp_quota_alert_threshold") {
            obj.insert(
                "ghcp_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("windsurf_quota_alert_enabled") {
            obj.insert(
                "windsurf_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("windsurf_quota_alert_threshold") {
            obj.insert(
                "windsurf_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("kiro_quota_alert_enabled") {
            obj.insert(
                "kiro_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("kiro_quota_alert_threshold") {
            obj.insert(
                "kiro_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("cursor_quota_alert_enabled") {
            obj.insert(
                "cursor_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("cursor_quota_alert_threshold") {
            obj.insert(
                "cursor_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("gemini_quota_alert_enabled") {
            obj.insert(
                "gemini_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("gemini_quota_alert_threshold") {
            obj.insert(
                "gemini_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("codebuddy_quota_alert_enabled") {
            obj.insert(
                "codebuddy_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("codebuddy_quota_alert_threshold") {
            obj.insert(
                "codebuddy_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("codebuddy_cn_quota_alert_enabled") {
            obj.insert(
                "codebuddy_cn_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("codebuddy_cn_quota_alert_threshold") {
            obj.insert(
                "codebuddy_cn_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("qoder_quota_alert_enabled") {
            obj.insert(
                "qoder_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("qoder_quota_alert_threshold") {
            obj.insert(
                "qoder_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("trae_quota_alert_enabled") {
            obj.insert(
                "trae_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("trae_quota_alert_threshold") {
            obj.insert(
                "trae_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
    }

    serde_json::from_value(value).map_err(|e| format!("解析配置文件失败: {}", e))
}

/// 保存用户配置
pub fn save_user_config(config: &UserConfig) -> Result<(), String> {
    let config_path = get_user_config_path()?;
    let data_dir = get_data_dir()?;

    // 确保目录存在
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir).map_err(|e| format!("创建配置目录失败: {}", e))?;
    }

    let json =
        serde_json::to_string_pretty(config).map_err(|e| format!("序列化配置失败: {}", e))?;

    fs::write(&config_path, json).map_err(|e| format!("写入配置文件失败: {}", e))?;

    // 更新运行时状态
    if let Ok(mut state) = get_runtime_state().write() {
        state.user_config = config.clone();
    }

    crate::modules::logger::log_info(&format!(
        "[Config] 用户配置已保存: ws_enabled={}, ws_port={}",
        config.ws_enabled, config.ws_port
    ));

    Ok(())
}

/// 获取用户配置（从内存）
pub fn get_user_config() -> UserConfig {
    get_runtime_state()
        .read()
        .map(|state| state.user_config.clone())
        .unwrap_or_default()
}

/// 获取用户配置的首选端口
pub fn get_preferred_port() -> u16 {
    get_user_config().ws_port
}

/// 获取当前实际使用的端口
pub fn get_actual_port() -> Option<u16> {
    get_runtime_state()
        .read()
        .ok()
        .and_then(|state| state.actual_port)
}

/// 保存服务状态到共享文件
pub fn save_server_status(status: &ServerStatus) -> Result<(), String> {
    let status_path = get_server_status_path()?;
    let data_dir = get_data_dir()?;

    // 确保目录存在
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir).map_err(|e| format!("创建配置目录失败: {}", e))?;
    }

    // 写入状态文件
    let json =
        serde_json::to_string_pretty(status).map_err(|e| format!("序列化状态失败: {}", e))?;

    fs::write(&status_path, json).map_err(|e| format!("写入状态文件失败: {}", e))?;

    crate::modules::logger::log_info(&format!(
        "[Config] 服务状态已保存: ws_port={}, pid={}",
        status.ws_port, status.pid
    ));

    Ok(())
}

/// 初始化服务状态（WebSocket 启动后调用）
pub fn init_server_status(actual_port: u16) -> Result<(), String> {
    // 更新运行时状态
    if let Ok(mut state) = get_runtime_state().write() {
        state.actual_port = Some(actual_port);
    }

    let status = ServerStatus {
        ws_port: actual_port,
        version: env!("CARGO_PKG_VERSION").to_string(),
        pid: std::process::id(),
        started_at: chrono::Utc::now().timestamp(),
    };

    save_server_status(&status)?;

    Ok(())
}
