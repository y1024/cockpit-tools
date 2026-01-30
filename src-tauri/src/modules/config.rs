//! 配置服务模块
//! 管理应用配置，包括 WebSocket 端口等

use serde::{Deserialize, Serialize};
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
    /// 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_auto_refresh")]
    pub auto_refresh_minutes: i32,
    /// Codex 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_codex_auto_refresh")]
    pub codex_auto_refresh_minutes: i32,
    /// 窗口关闭行为
    #[serde(default = "default_close_behavior")]
    pub close_behavior: CloseWindowBehavior,
    /// OpenCode 启动路径（为空则使用默认路径）
    #[serde(default = "default_opencode_app_path")]
    pub opencode_app_path: String,
    /// 切换 Codex 时是否自动重启 OpenCode
    #[serde(default = "default_opencode_sync_on_switch")]
    pub opencode_sync_on_switch: bool,
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

fn default_ws_enabled() -> bool { true }
fn default_ws_port() -> u16 { DEFAULT_WS_PORT }
fn default_language() -> String { "zh-cn".to_string() }
fn default_theme() -> String { "system".to_string() }
fn default_auto_refresh() -> i32 { 10 } // 默认 10 分钟
fn default_codex_auto_refresh() -> i32 { 10 } // 默认 10 分钟
fn default_close_behavior() -> CloseWindowBehavior { CloseWindowBehavior::Ask }
fn default_opencode_app_path() -> String { String::new() }
fn default_opencode_sync_on_switch() -> bool { true }

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            ws_enabled: true,
            ws_port: DEFAULT_WS_PORT,
            language: default_language(),
            theme: default_theme(),
            auto_refresh_minutes: default_auto_refresh(),
            codex_auto_refresh_minutes: default_codex_auto_refresh(),
            close_behavior: default_close_behavior(),
            opencode_app_path: default_opencode_app_path(),
            opencode_sync_on_switch: default_opencode_sync_on_switch(),
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
    
    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("读取配置文件失败: {}", e))?;
    
    serde_json::from_str(&content)
        .map_err(|e| format!("解析配置文件失败: {}", e))
}

/// 保存用户配置
pub fn save_user_config(config: &UserConfig) -> Result<(), String> {
    let config_path = get_user_config_path()?;
    let data_dir = get_data_dir()?;
    
    // 确保目录存在
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir)
            .map_err(|e| format!("创建配置目录失败: {}", e))?;
    }
    
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("序列化配置失败: {}", e))?;
    
    fs::write(&config_path, json)
        .map_err(|e| format!("写入配置文件失败: {}", e))?;
    
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
        fs::create_dir_all(&data_dir)
            .map_err(|e| format!("创建配置目录失败: {}", e))?;
    }
    
    // 写入状态文件
    let json = serde_json::to_string_pretty(status)
        .map_err(|e| format!("序列化状态失败: {}", e))?;
    
    fs::write(&status_path, json)
        .map_err(|e| format!("写入状态文件失败: {}", e))?;
    
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
