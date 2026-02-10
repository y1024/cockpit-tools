use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::{Command, Stdio};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Child;
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{Pid, System};
use crate::modules::config;

const OPENCODE_APP_NAME: &str = "OpenCode";
#[cfg(target_os = "macos")]
const CODEX_APP_PATH: &str = "/Applications/Codex.app/Contents/MacOS/Codex";
#[cfg(target_os = "macos")]
const ANTIGRAVITY_APP_PATH: &str = "/Applications/Antigravity.app/Contents/MacOS/Electron";
#[cfg(target_os = "macos")]
const VSCODE_APP_PATH: &str = "/Applications/Visual Studio Code.app/Contents/MacOS/Electron";

#[cfg(target_os = "windows")]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
#[cfg(target_os = "windows")]
const DETACHED_PROCESS: u32 = 0x0000_0008;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(target_os = "windows")]
fn powershell_output(args: &[&str]) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;

    Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-WindowStyle", "Hidden", "-NonInteractive", "-NoProfile"])
        .args(args)
        .output()
}

fn should_detach_child() -> bool {
    if let Ok(value) = std::env::var("COCKPIT_CHILD_LOGS") {
        let lowered = value.trim().to_lowercase();
        if matches!(lowered.as_str(), "1" | "true" | "yes" | "on") {
            return false;
        }
    }
    if let Ok(value) = std::env::var("COCKPIT_CHILD_DETACH") {
        let lowered = value.trim().to_lowercase();
        if matches!(lowered.as_str(), "0" | "false" | "no" | "off") {
            return false;
        }
    }
    true
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn spawn_detached_unix(cmd: &mut Command) -> Result<Child, String> {
    use std::os::unix::process::CommandExt;
    if !should_detach_child() {
        return cmd.spawn().map_err(|e| format!("启动失败: {}", e));
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.spawn().map_err(|e| format!("启动失败: {}", e))
}

fn normalize_custom_path(value: Option<&str>) -> Option<String> {
    let trimmed = value.unwrap_or("").trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

const APP_PATH_NOT_FOUND_PREFIX: &str = "APP_PATH_NOT_FOUND:";

fn app_path_missing_error(app: &str) -> String {
    format!("{}{}", APP_PATH_NOT_FOUND_PREFIX, app)
}

#[cfg(target_os = "macos")]
fn normalize_macos_app_root(path: &Path) -> Option<String> {
    let path_str = path.to_string_lossy();
    if let Some(app_idx) = path_str.find(".app") {
        return Some(path_str[..app_idx + 4].to_string());
    }
    None
}

#[cfg(target_os = "macos")]
fn resolve_macos_exec_path(path_str: &str, binary_name: &str) -> Option<std::path::PathBuf> {
    let path = std::path::PathBuf::from(path_str);
    if let Some(app_root) = normalize_macos_app_root(&path) {
        let exec_path = std::path::PathBuf::from(&app_root)
            .join("Contents")
            .join("MacOS")
            .join(binary_name);
        if exec_path.exists() {
            return Some(exec_path);
        }
    }
    if path.exists() {
        return Some(path);
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn resolve_macos_exec_path(path_str: &str, _binary_name: &str) -> Option<std::path::PathBuf> {
    let path = std::path::PathBuf::from(path_str);
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

fn update_app_path_in_config(app: &str, path: &Path) {
    let mut current = config::get_user_config();
    let normalized = {
        #[cfg(target_os = "macos")]
        {
            normalize_macos_app_root(path).unwrap_or_else(|| path.to_string_lossy().to_string())
        }
        #[cfg(not(target_os = "macos"))]
        {
            path.to_string_lossy().to_string()
        }
    };
    match app {
        "antigravity" => {
            if current.antigravity_app_path != normalized {
                current.antigravity_app_path = normalized;
            } else {
                return;
            }
        }
        "codex" => {
            if current.codex_app_path != normalized {
                current.codex_app_path = normalized;
            } else {
                return;
            }
        }
        "vscode" => {
            if current.vscode_app_path != normalized {
                current.vscode_app_path = normalized;
            } else {
                return;
            }
        }
        "opencode" => {
            if current.opencode_app_path != normalized {
                current.opencode_app_path = normalized;
            } else {
                return;
            }
        }
        _ => return,
    }
    let _ = config::save_user_config(&current);
}

#[cfg(target_os = "macos")]
fn resolve_macos_app_root_from_config(app: &str) -> Option<String> {
    let current = config::get_user_config();
    let raw = match app {
        "antigravity" => current.antigravity_app_path,
        "codex" => current.codex_app_path,
        "vscode" => current.vscode_app_path,
        _ => String::new(),
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = std::path::Path::new(trimmed);
    let app_root = normalize_macos_app_root(path)?;
    if std::path::Path::new(&app_root).exists() {
        return Some(app_root);
    }
    None
}

#[cfg(target_os = "macos")]
fn spawn_open_app(app_root: &str, args: &[String]) -> Result<u32, String> {
    let mut cmd = Command::new("open");
    cmd.arg("-a").arg(app_root);
    if !args.is_empty() {
        cmd.arg("--args");
        for arg in args {
            if !arg.trim().is_empty() {
                cmd.arg(arg);
            }
        }
    }
    let child = spawn_detached_unix(&mut cmd).map_err(|e| format!("启动失败: {}", e))?;
    Ok(child.id())
}

fn find_antigravity_process_exe() -> Option<std::path::PathBuf> {
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let current_pid = std::process::id();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        if pid_u32 == current_pid {
            continue;
        }

        let name = process.name().to_string_lossy().to_lowercase();
        let exe_path = process
            .exe()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_lowercase();

        let args = process.cmd();
        let args_str = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_lowercase())
            .collect::<Vec<String>>()
            .join(" ");

        let is_helper = args_str.contains("--type=")
            || name.contains("helper")
            || name.contains("plugin")
            || name.contains("renderer")
            || name.contains("gpu")
            || name.contains("crashpad")
            || name.contains("utility")
            || name.contains("audio")
            || name.contains("sandbox")
            || exe_path.contains("crashpad");

        #[cfg(target_os = "macos")]
        let is_antigravity =
            exe_path.contains("antigravity.app") && !exe_path.contains("antigravity tools.app");
        #[cfg(target_os = "windows")]
        let is_antigravity = name == "antigravity.exe" || exe_path.ends_with("\\antigravity.exe");
        #[cfg(target_os = "linux")]
        let is_antigravity = (name.contains("antigravity") || exe_path.contains("/antigravity"))
            && !name.contains("tools")
            && !exe_path.contains("tools");

        if is_antigravity && !is_helper {
            if let Some(exe) = process.exe() {
                return Some(exe.to_path_buf());
            }
        }
    }

    None
}

fn find_vscode_process_exe() -> Option<std::path::PathBuf> {
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let current_pid = std::process::id();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        if pid_u32 == current_pid {
            continue;
        }

        let name = process.name().to_string_lossy().to_lowercase();
        let exe_path = process
            .exe()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_lowercase();

        let args = process.cmd();
        let args_str = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_lowercase())
            .collect::<Vec<String>>()
            .join(" ");

        let is_helper = args_str.contains("--type=")
            || name.contains("helper")
            || name.contains("renderer")
            || name.contains("gpu")
            || name.contains("crashpad")
            || name.contains("utility")
            || name.contains("audio")
            || name.contains("sandbox");

        #[cfg(target_os = "macos")]
        let is_vscode = exe_path.contains("visual studio code.app/contents/") && !is_helper;
        #[cfg(target_os = "windows")]
        let is_vscode = (name == "code.exe" || exe_path.ends_with("\\code.exe")) && !is_helper;
        #[cfg(target_os = "linux")]
        let is_vscode = (name == "code" || exe_path.ends_with("/code")) && !is_helper;

        if is_vscode {
            if let Some(exe) = process.exe() {
                return Some(exe.to_path_buf());
            }
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn find_codex_process_exe() -> Option<std::path::PathBuf> {
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let current_pid = std::process::id();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        if pid_u32 == current_pid {
            continue;
        }

        let name = process.name().to_string_lossy().to_lowercase();
        let exe_path = process
            .exe()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_lowercase();

        let args = process.cmd();
        let args_str = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_lowercase())
            .collect::<Vec<String>>()
            .join(" ");

        let is_helper = args_str.contains("--type=")
            || name.contains("helper")
            || name.contains("renderer")
            || name.contains("gpu")
            || name.contains("crashpad")
            || name.contains("utility")
            || name.contains("audio")
            || name.contains("sandbox");

        let is_codex = exe_path.contains("codex.app/contents/macos/codex");

        if is_codex && !is_helper {
            if let Some(exe) = process.exe() {
                return Some(exe.to_path_buf());
            }
        }
    }

    None
}

fn detect_antigravity_exec_path() -> Option<std::path::PathBuf> {
    if let Some(path) = find_antigravity_process_exe() {
        return Some(path);
    }

    #[cfg(target_os = "macos")]
    {
        let path = std::path::PathBuf::from(ANTIGRAVITY_APP_PATH);
        if path.exists() {
            return Some(path);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            candidates.push(
                std::path::PathBuf::from(local_appdata)
                    .join("Programs")
                    .join("Antigravity")
                    .join("Antigravity.exe"),
            );
        }
        if let Ok(program_files) = std::env::var("PROGRAMFILES") {
            candidates.push(
                std::path::PathBuf::from(program_files)
                    .join("Antigravity")
                    .join("Antigravity.exe"),
            );
        }
        if let Ok(program_files_x86) = std::env::var("PROGRAMFILES(X86)") {
            candidates.push(
                std::path::PathBuf::from(program_files_x86)
                    .join("Antigravity")
                    .join("Antigravity.exe"),
            );
        }
        for candidate in candidates {
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let candidates = [
            "/usr/bin/antigravity",
            "/opt/antigravity/antigravity",
            "/usr/share/antigravity/antigravity",
        ];
        for candidate in candidates {
            let path = std::path::PathBuf::from(candidate);
            if path.exists() {
                return Some(path);
            }
        }
        if let Some(home) = dirs::home_dir() {
            let user_local = home.join(".local/bin/antigravity");
            if user_local.exists() {
                return Some(user_local);
            }
        }
    }

    None
}

fn detect_vscode_exec_path() -> Option<std::path::PathBuf> {
    if let Some(path) = find_vscode_process_exe() {
        return Some(path);
    }

    #[cfg(target_os = "macos")]
    {
        let path = std::path::PathBuf::from(VSCODE_APP_PATH);
        if path.exists() {
            return Some(path);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            candidates.push(
                std::path::PathBuf::from(&local_appdata)
                    .join("Programs")
                    .join("Microsoft VS Code")
                    .join("Code.exe"),
            );
            candidates.push(
                std::path::PathBuf::from(&local_appdata)
                    .join("Programs")
                    .join("VSCode")
                    .join("Code.exe"),
            );
        }
        if let Ok(program_files) = std::env::var("PROGRAMFILES") {
            candidates.push(
                std::path::PathBuf::from(program_files)
                    .join("Microsoft VS Code")
                    .join("Code.exe"),
            );
        }
        if let Ok(program_files_x86) = std::env::var("PROGRAMFILES(X86)") {
            candidates.push(
                std::path::PathBuf::from(program_files_x86)
                    .join("Microsoft VS Code")
                    .join("Code.exe"),
            );
        }
        for candidate in candidates {
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let candidates = [
            "/usr/bin/code",
            "/snap/bin/code",
            "/var/lib/flatpak/exports/bin/com.visualstudio.code",
            "/usr/local/bin/code",
        ];
        for candidate in candidates {
            let path = std::path::PathBuf::from(candidate);
            if path.exists() {
                return Some(path);
            }
        }
        if let Some(home) = dirs::home_dir() {
            let user_local = home.join(".local/bin/code");
            if user_local.exists() {
                return Some(user_local);
            }
        }
    }

    None
}

fn detect_codex_exec_path() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        if let Some(path) = find_codex_process_exe() {
            return Some(path);
        }
        let path = std::path::PathBuf::from(CODEX_APP_PATH);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

fn detect_opencode_exec_path() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let candidate = std::path::PathBuf::from("/Applications/OpenCode.app");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            candidates.push(
                std::path::PathBuf::from(local_appdata)
                    .join("Programs")
                    .join("OpenCode")
                    .join("OpenCode.exe"),
            );
        }
        if let Ok(program_files) = std::env::var("PROGRAMFILES") {
            candidates.push(
                std::path::PathBuf::from(program_files)
                    .join("OpenCode")
                    .join("OpenCode.exe"),
            );
        }
        for candidate in candidates {
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let candidates = [
            "/usr/bin/opencode",
            "/opt/opencode/opencode",
        ];
        for candidate in candidates {
            let path = std::path::PathBuf::from(candidate);
            if path.exists() {
                return Some(path);
            }
        }
    }

    None
}

fn resolve_antigravity_launch_path() -> Result<std::path::PathBuf, String> {
    if let Some(custom) = normalize_custom_path(Some(&config::get_user_config().antigravity_app_path)) {
        if let Some(exec) = resolve_macos_exec_path(&custom, "Electron") {
            return Ok(exec);
        }
    }

    if let Some(detected) = detect_antigravity_exec_path() {
        update_app_path_in_config("antigravity", &detected);
        return Ok(detected);
    }

    Err(app_path_missing_error("antigravity"))
}

fn resolve_vscode_launch_path() -> Result<std::path::PathBuf, String> {
    if let Some(custom) = normalize_custom_path(Some(&config::get_user_config().vscode_app_path)) {
        #[cfg(target_os = "macos")]
        {
            if let Some(exec) = resolve_macos_exec_path(&custom, "Electron") {
                return Ok(exec);
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            if let Some(exec) = resolve_macos_exec_path(&custom, "Electron") {
                return Ok(exec);
            }
        }
    }

    if let Some(detected) = detect_vscode_exec_path() {
        update_app_path_in_config("vscode", &detected);
        return Ok(detected);
    }

    Err(app_path_missing_error("vscode"))
}

#[cfg(target_os = "macos")]
fn resolve_codex_launch_path() -> Result<std::path::PathBuf, String> {
    if let Some(custom) = normalize_custom_path(Some(&config::get_user_config().codex_app_path)) {
        if let Some(exec) = resolve_macos_exec_path(&custom, "Codex") {
            return Ok(exec);
        }
    }

    if let Some(detected) = detect_codex_exec_path() {
        update_app_path_in_config("codex", &detected);
        return Ok(detected);
    }

    Err(app_path_missing_error("codex"))
}

pub fn detect_and_save_app_path(app: &str) -> Option<String> {
    let current = config::get_user_config();
    match app {
        "antigravity" => {
            if !current.antigravity_app_path.trim().is_empty() {
                return Some(current.antigravity_app_path);
            }
            if let Some(detected) = detect_antigravity_exec_path() {
                update_app_path_in_config("antigravity", &detected);
                return Some(config::get_user_config().antigravity_app_path);
            }
        }
        "codex" => {
            if !current.codex_app_path.trim().is_empty() {
                return Some(current.codex_app_path);
            }
            if let Some(detected) = detect_codex_exec_path() {
                update_app_path_in_config("codex", &detected);
                return Some(config::get_user_config().codex_app_path);
            }
        }
        "vscode" => {
            if !current.vscode_app_path.trim().is_empty() {
                return Some(current.vscode_app_path);
            }
            if let Some(detected) = detect_vscode_exec_path() {
                update_app_path_in_config("vscode", &detected);
                return Some(config::get_user_config().vscode_app_path);
            }
        }
        "opencode" => {
            if !current.opencode_app_path.trim().is_empty() {
                return Some(current.opencode_app_path);
            }
            if let Some(detected) = detect_opencode_exec_path() {
                update_app_path_in_config("opencode", &detected);
                return Some(config::get_user_config().opencode_app_path);
            }
        }
        _ => {}
    }
    None
}

/// 检查 Antigravity 是否在运行
#[allow(dead_code)]
pub fn is_antigravity_running() -> bool {
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let current_pid = std::process::id();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        if pid_u32 == current_pid {
            continue;
        }

        let name = process.name().to_string_lossy().to_lowercase();
        let exe_path = process
            .exe()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_lowercase();

        // 通用的辅助进程排除逻辑
        let args = process.cmd();
        let args_str = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_lowercase())
            .collect::<Vec<String>>()
            .join(" ");

        let is_helper = args_str.contains("--type=")
            || name.contains("helper")
            || name.contains("plugin")
            || name.contains("renderer")
            || name.contains("gpu")
            || name.contains("crashpad")
            || name.contains("utility")
            || name.contains("audio")
            || name.contains("sandbox")
            || exe_path.contains("crashpad");

        #[cfg(target_os = "macos")]
        {
            if exe_path.contains("antigravity.app") && !is_helper {
                return true;
            }
        }

        #[cfg(target_os = "windows")]
        {
            if name == "antigravity.exe" && !is_helper {
                return true;
            }
        }

        #[cfg(target_os = "linux")]
        {
            if (name.contains("antigravity") || exe_path.contains("/antigravity"))
                && !name.contains("tools")
                && !is_helper
            {
                return true;
            }
        }
    }

    false
}

pub fn is_pid_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    system.process(Pid::from(pid as usize)).is_some()
}


#[allow(dead_code)]
fn extract_user_data_dir(args: &[std::ffi::OsString]) -> Option<String> {
    let tokens: Vec<String> = args.iter().map(|arg| arg.to_string_lossy().to_string()).collect();
    let mut index = 0;
    while index < tokens.len() {
        let value = tokens[index].as_str();
        if let Some(rest) = value.strip_prefix("--user-data-dir=") {
            return Some(rest.to_string());
        }
        if value == "--user-data-dir" {
            index += 1;
            if index >= tokens.len() {
                return None;
            }
            let mut parts = Vec::new();
            while index < tokens.len() {
                let part = tokens[index].as_str();
                if part.starts_with("--") {
                    break;
                }
                parts.push(part);
                index += 1;
            }
            if !parts.is_empty() {
                return Some(parts.join(" "));
            }
            return None;
        }
        index += 1;
    }
    None
}


#[allow(dead_code)]
fn parse_user_data_dir_value(raw: &str) -> Option<String> {
    let rest = raw.trim_start();
    if rest.is_empty() {
        return None;
    }
    let value = if rest.starts_with('"') {
        let end = rest[1..].find('"').map(|idx| idx + 1).unwrap_or(rest.len());
        &rest[1..end]
    } else if rest.starts_with('\'') {
        let end = rest[1..].find('\'').map(|idx| idx + 1).unwrap_or(rest.len());
        &rest[1..end]
    } else {
        let end = rest.find(" --").unwrap_or(rest.len());
        &rest[..end]
    };
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}


#[allow(dead_code)]
fn extract_user_data_dir_from_command_line(command_line: &str) -> Option<String> {
    let tokens = split_command_tokens(command_line);
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index].as_str();
        if let Some(rest) = token.strip_prefix("--user-data-dir=") {
            if !rest.trim().is_empty() {
                return Some(rest.to_string());
            }
        }
        if token == "--user-data-dir" {
            index += 1;
            if index >= tokens.len() {
                return None;
            }
            let mut parts = Vec::new();
            while index < tokens.len() {
                let part = tokens[index].as_str();
                if part.starts_with("--") || is_env_token(part) {
                    break;
                }
                parts.push(part);
                index += 1;
            }
            if !parts.is_empty() {
                return Some(parts.join(" "));
            }
            return None;
        }
        index += 1;
    }
    None
}

#[cfg(target_os = "macos")]
fn parse_env_value(raw: &str) -> Option<String> {
    let rest = raw.trim_start();
    if rest.is_empty() {
        return None;
    }
    let value = if rest.starts_with('"') {
        let end = rest[1..].find('"').map(|idx| idx + 1).unwrap_or(rest.len());
        &rest[1..end]
    } else if rest.starts_with('\'') {
        let end = rest[1..].find('\'').map(|idx| idx + 1).unwrap_or(rest.len());
        &rest[1..end]
    } else {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        &rest[..end]
    };
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(target_os = "macos")]
fn extract_env_value_from_tokens(tokens: &[String], key: &str) -> Option<String> {
    if tokens.is_empty() {
        return None;
    }
    let prefix = format!("{}=", key);
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index].as_str();
        if let Some(rest) = token.strip_prefix(&prefix) {
            let mut parts: Vec<&str> = Vec::new();
            if !rest.is_empty() {
                parts.push(rest);
            }
            let mut next = index + 1;
            while next < tokens.len() {
                let value = tokens[next].as_str();
                if value.starts_with("--") || is_env_token(value) {
                    break;
                }
                parts.push(value);
                next += 1;
            }
            if parts.is_empty() {
                return None;
            }
            let joined = parts.join(" ");
            let trimmed = joined.trim();
            if trimmed.is_empty() {
                return None;
            }
            return Some(trimmed.to_string());
        }
        index += 1;
    }
    None
}

fn split_command_tokens(command_line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for ch in command_line.chars() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            None => {
                if ch == '"' || ch == '\'' {
                    quote = Some(ch);
                } else if ch.is_whitespace() {
                    if !current.is_empty() {
                        tokens.push(current.clone());
                        current.clear();
                    }
                } else {
                    current.push(ch);
                }
            }
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn is_env_token(token: &str) -> bool {
    let (key, _) = match token.split_once('=') {
        Some(parts) => parts,
        None => return false,
    };
    if key.is_empty() {
        return false;
    }
    let mut chars = key.chars();
    let first = match chars.next() {
        Some(value) => value,
        None => return false,
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(target_os = "macos")]
fn extract_env_value(command_line: &str, key: &str) -> Option<String> {
    let needle = format!("{}=", key);
    let pos = command_line.find(&needle)?;
    let rest = &command_line[pos + needle.len()..];
    parse_env_value(rest)
}


#[allow(dead_code)]
fn normalize_path_for_compare(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let resolved = std::fs::canonicalize(trimmed)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| trimmed.to_string());

    #[cfg(target_os = "windows")]
    {
        return resolved.to_lowercase();
    }
    #[cfg(not(target_os = "windows"))]
    {
        return resolved;
    }
}

fn is_helper_command_line(cmdline_lower: &str) -> bool {
    cmdline_lower.contains("--type=")
        || cmdline_lower.contains("helper")
        || cmdline_lower.contains("plugin")
        || cmdline_lower.contains("renderer")
        || cmdline_lower.contains("gpu")
        || cmdline_lower.contains("crashpad")
        || cmdline_lower.contains("utility")
        || cmdline_lower.contains("audio")
        || cmdline_lower.contains("sandbox")
}

#[cfg(target_os = "macos")]
fn collect_antigravity_process_entries_from_ps() -> Vec<(u32, Option<String>)> {
    let mut result = Vec::new();
    let output = Command::new("ps").args(["-axo", "pid,command"]).output();
    let output = match output {
        Ok(value) => value,
        Err(_) => return result,
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, |ch: char| ch.is_whitespace());
        let pid_str = parts.next().unwrap_or("").trim();
        let cmdline = parts.next().unwrap_or("").trim();
        let pid = match pid_str.parse::<u32>() {
            Ok(value) => value,
            Err(_) => continue,
        };
        let lower = cmdline.to_lowercase();
        if !lower.contains("antigravity.app/contents/") {
            continue;
        }
        if lower.contains("antigravity tools.app/contents/")
            || lower.contains("crashpad_handler")
            || is_helper_command_line(&lower)
        {
            continue;
        }
        let dir = extract_user_data_dir_from_command_line(cmdline);
        result.push((pid, dir));
    }
    result
}

#[cfg(target_os = "windows")]
fn collect_antigravity_process_entries_from_powershell() -> Vec<(u32, Option<String>)> {
    let mut result = Vec::new();
    let output = powershell_output(&[
        "-NoProfile",
        "-Command",
        "Get-CimInstance Win32_Process -Filter \"Name='Antigravity.exe'\" | ForEach-Object { \"$($_.ProcessId)|$($_.CommandLine)\" }",
    ]);
    let output = match output {
        Ok(value) => value,
        Err(_) => return result,
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, '|');
        let pid_str = parts.next().unwrap_or("").trim();
        let cmdline = parts.next().unwrap_or("").trim();
        let pid = match pid_str.parse::<u32>() {
            Ok(value) => value,
            Err(_) => continue,
        };
        let lower = cmdline.to_lowercase();
        if lower.contains("antigravity tools") || is_helper_command_line(&lower) {
            continue;
        }
        let dir = extract_user_data_dir_from_command_line(cmdline);
        result.push((pid, dir));
    }
    result
}

#[cfg(target_os = "linux")]
fn collect_antigravity_process_entries_from_proc() -> Vec<(u32, Option<String>)> {
    let mut result = Vec::new();
    let entries = match std::fs::read_dir("/proc") {
        Ok(value) => value,
        Err(_) => return result,
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let pid_str = file_name.to_string_lossy();
        if !pid_str.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let pid = match pid_str.parse::<u32>() {
            Ok(value) => value,
            Err(_) => continue,
        };
        let cmdline_path = format!("/proc/{}/cmdline", pid);
        let cmdline = match std::fs::read(&cmdline_path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if cmdline.is_empty() {
            continue;
        }
        let cmdline_str = String::from_utf8_lossy(&cmdline).replace('\0', " ");
        let cmd_lower = cmdline_str.to_lowercase();
        let exe_path = std::fs::read_link(format!("/proc/{}/exe", pid))
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_lowercase()))
            .unwrap_or_default();
        if !cmd_lower.contains("antigravity") && !exe_path.contains("antigravity") {
            continue;
        }
        if cmd_lower.contains("tools") || exe_path.contains("tools") {
            continue;
        }
        if is_helper_command_line(&cmd_lower) {
            continue;
        }
        let dir = extract_user_data_dir_from_command_line(&cmdline_str);
        result.push((pid, dir));
    }
    result
}

pub fn collect_antigravity_process_entries() -> Vec<(u32, Option<String>)> {
    #[cfg(target_os = "macos")]
    {
        let entries = collect_antigravity_process_entries_macos();
        if !entries.is_empty() {
            return entries;
        }
        let entries = collect_antigravity_process_entries_from_ps();
        if !entries.is_empty() {
            return entries;
        }
    }

    #[cfg(target_os = "windows")]
    {
        let entries = collect_antigravity_process_entries_from_powershell();
        if !entries.is_empty() {
            return entries;
        }
    }

    #[cfg(target_os = "linux")]
    {
        let entries = collect_antigravity_process_entries_from_proc();
        if !entries.is_empty() {
            return entries;
        }
    }

    let mut result = Vec::new();
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let current_pid = std::process::id();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        if pid_u32 == current_pid {
            continue;
        }

        #[cfg(target_os = "macos")]
        let _name = process.name().to_string_lossy().to_lowercase();
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let name = process.name().to_string_lossy().to_lowercase();
        let exe_path = process
            .exe()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_lowercase();

        #[cfg(target_os = "macos")]
        let is_antigravity =
            exe_path.contains("antigravity.app") && !exe_path.contains("antigravity tools.app");
        #[cfg(target_os = "windows")]
        let is_antigravity = name == "antigravity.exe" || exe_path.ends_with("\\antigravity.exe");
        #[cfg(target_os = "linux")]
        let is_antigravity = (name.contains("antigravity") || exe_path.contains("/antigravity"))
            && !name.contains("tools")
            && !exe_path.contains("tools");

        if !is_antigravity {
            continue;
        }

        let args = process.cmd();
        let dir = extract_user_data_dir(&args);
        result.push((pid_u32, dir));
    }

    result
}

fn pick_preferred_pid(mut pids: Vec<u32>) -> Option<u32> {
    if pids.is_empty() {
        return None;
    }
    pids.sort();
    pids.dedup();
    pids.first().copied()
}

pub fn resolve_antigravity_pid_from_entries(
    last_pid: Option<u32>,
    user_data_dir: Option<&str>,
    entries: &[(u32, Option<String>)],
) -> Option<u32> {
    if let Some(pid) = last_pid {
        if is_pid_running(pid) {
            return Some(pid);
        }
    }

    let target = user_data_dir
        .map(|value| normalize_path_for_compare(value))
        .filter(|value| !value.is_empty());

    let mut matches = Vec::new();
    for (pid, dir) in entries {
        match (&target, dir.as_ref()) {
            (Some(target_dir), Some(dir)) => {
                let normalized = normalize_path_for_compare(dir);
                if !normalized.is_empty() && &normalized == target_dir {
                    matches.push(*pid);
                }
            }
            (None, None) => {
                matches.push(*pid);
            }
            (None, Some(dir)) => {
                let normalized = normalize_path_for_compare(dir);
                if normalized.is_empty() {
                    matches.push(*pid);
                }
            }
            _ => {}
        }
    }

    pick_preferred_pid(matches)
}

pub fn resolve_antigravity_pid(last_pid: Option<u32>, user_data_dir: Option<&str>) -> Option<u32> {
    if let Some(pid) = last_pid {
        if is_pid_running(pid) {
            return Some(pid);
        }
    }
    let entries = collect_antigravity_process_entries();
    resolve_antigravity_pid_from_entries(None, user_data_dir, &entries)
}

#[cfg(target_os = "macos")]
fn focus_window_by_pid(pid: u32) -> Result<(), String> {
    let script = format!(
        "tell application \"System Events\" to set frontmost of (first process whose unix id is {}) to true",
        pid
    );
    crate::modules::logger::log_info(&format!(
        "[Focus] macOS osascript start pid={}",
        pid
    ));
    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("调用 osascript 失败: {}", e))?;
    if output.status.success() {
        crate::modules::logger::log_info(&format!(
            "[Focus] macOS osascript success pid={}",
            pid
        ));
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "窗口聚焦失败，请检查系统辅助功能权限: {}",
        stderr.trim()
    ))
}

#[cfg(target_os = "windows")]
fn focus_window_by_pid(pid: u32) -> Result<(), String> {
    let command = format!(
        r#"$pid={pid};$p=Get-Process -Id $pid -ErrorAction Stop;$h=$p.MainWindowHandle;if ($h -eq 0) {{ throw 'MAIN_WINDOW_HANDLE_EMPTY' }};Add-Type @' 
using System; 
using System.Runtime.InteropServices; 
public class Win32 {{ 
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd); 
  [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow); 
}} 
'@;[Win32]::ShowWindowAsync($h, 9) | Out-Null;[Win32]::SetForegroundWindow($h) | Out-Null;"#
    );
    crate::modules::logger::log_info(&format!(
        "[Focus] Windows PowerShell start pid={}",
        pid
    ));
    let output = powershell_output(&["-NoProfile", "-Command", &command])
        .map_err(|e| format!("调用 PowerShell 失败: {}", e))?;
    if output.status.success() {
        crate::modules::logger::log_info(&format!(
            "[Focus] Windows PowerShell success pid={}",
            pid
        ));
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!("窗口聚焦失败: {}", stderr.trim()))
}

#[cfg(target_os = "linux")]
fn focus_window_by_pid(pid: u32) -> Result<(), String> {
    if let Ok(output) = Command::new("wmctrl").arg("-lp").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let mut parts = line.split_whitespace();
                let win_id = parts.next();
                let _desktop = parts.next();
                let pid_str = parts.next();
                if let (Some(win_id), Some(pid_str)) = (win_id, pid_str) {
                    if pid_str == pid.to_string() {
                        let focus = Command::new("wmctrl")
                            .args(["-ia", win_id])
                            .output();
                        if let Ok(focus) = focus {
                            if focus.status.success() {
                                crate::modules::logger::log_info(&format!(
                                    "[Focus] Linux wmctrl success pid={}",
                                    pid
                                ));
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
    }

    crate::modules::logger::log_info(&format!(
        "[Focus] Linux wmctrl not available or failed, trying xdotool pid={}",
        pid
    ));
    let output = Command::new("xdotool")
        .args(["search", "--pid", &pid.to_string(), "windowactivate"])
        .output()
        .map_err(|e| format!("调用 xdotool 失败: {}", e))?;
    if output.status.success() {
        crate::modules::logger::log_info(&format!(
            "[Focus] Linux xdotool success pid={}",
            pid
        ));
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!("窗口聚焦失败: {}", stderr.trim()))
}

pub fn focus_antigravity_instance(
    last_pid: Option<u32>,
    user_data_dir: Option<&str>,
) -> Result<u32, String> {
    let resolve_start = Instant::now();
    let pid = resolve_antigravity_pid(last_pid, user_data_dir)
        .ok_or_else(|| "实例未运行，无法定位窗口".to_string())?;
    crate::modules::logger::log_info(&format!(
        "[Focus] Antigravity resolve pid={} elapsed={}ms",
        pid,
        resolve_start.elapsed().as_millis()
    ));
    let focus_start = Instant::now();
    focus_window_by_pid(pid)?;
    crate::modules::logger::log_info(&format!(
        "[Focus] Antigravity focus pid={} elapsed={}ms",
        pid,
        focus_start.elapsed().as_millis()
    ));
    Ok(pid)
}

#[cfg(target_os = "macos")]
pub fn resolve_codex_pid_from_entries(
    last_pid: Option<u32>,
    codex_home: Option<&str>,
    entries: &[(u32, Option<String>)],
) -> Option<u32> {
    if let Some(pid) = last_pid {
        if is_pid_running(pid) {
            return Some(pid);
        }
    }

    let target = codex_home
        .map(|value| normalize_path_for_compare(value))
        .filter(|value| !value.is_empty());

    let mut matches = Vec::new();
    for (pid, home) in entries {
        match (&target, home.as_ref()) {
            (Some(target_home), Some(home)) => {
                let normalized = normalize_path_for_compare(home);
                if !normalized.is_empty() && &normalized == target_home {
                    matches.push(*pid);
                }
            }
            (None, None) => {
                matches.push(*pid);
            }
            (None, Some(home)) => {
                let normalized = normalize_path_for_compare(home);
                if normalized.is_empty() {
                    matches.push(*pid);
                }
            }
            _ => {}
        }
    }

    pick_preferred_pid(matches)
}

#[cfg(not(target_os = "macos"))]
pub fn resolve_codex_pid_from_entries(
    _last_pid: Option<u32>,
    _codex_home: Option<&str>,
    _entries: &[(u32, Option<String>)],
) -> Option<u32> {
    None
}

#[cfg(target_os = "macos")]
pub fn resolve_codex_pid(last_pid: Option<u32>, codex_home: Option<&str>) -> Option<u32> {
    let entries = collect_codex_process_entries();
    resolve_codex_pid_from_entries(last_pid, codex_home, &entries)
}

#[cfg(not(target_os = "macos"))]
pub fn resolve_codex_pid(_last_pid: Option<u32>, _codex_home: Option<&str>) -> Option<u32> {
    None
}

pub fn focus_codex_instance(last_pid: Option<u32>, codex_home: Option<&str>) -> Result<u32, String> {
    let resolve_start = Instant::now();
    let pid =
        resolve_codex_pid(last_pid, codex_home).ok_or_else(|| "实例未运行，无法定位窗口".to_string())?;
    crate::modules::logger::log_info(&format!(
        "[Focus] Codex resolve pid={} elapsed={}ms",
        pid,
        resolve_start.elapsed().as_millis()
    ));
    let focus_start = Instant::now();
    focus_window_by_pid(pid)?;
    crate::modules::logger::log_info(&format!(
        "[Focus] Codex focus pid={} elapsed={}ms",
        pid,
        focus_start.elapsed().as_millis()
    ));
    Ok(pid)
}

pub fn collect_vscode_process_entries() -> Vec<(u32, Option<String>)> {
    let mut entries = Vec::new();
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let current_pid = std::process::id();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        if pid_u32 == current_pid {
            continue;
        }

        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let name = process.name().to_string_lossy().to_lowercase();
        let exe_path = process
            .exe()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_lowercase();

        let args_str = process
            .cmd()
            .iter()
            .map(|arg| arg.to_string_lossy().to_lowercase())
            .collect::<Vec<String>>()
            .join(" ");
        let is_helper = is_helper_command_line(&args_str) || args_str.contains("crashpad");

        #[cfg(target_os = "macos")]
        let is_vscode = exe_path.contains("visual studio code.app/contents/macos/");
        #[cfg(target_os = "windows")]
        let is_vscode = name == "code.exe" || exe_path.ends_with("\\code.exe");
        #[cfg(target_os = "linux")]
        let is_vscode = name == "code" || exe_path.ends_with("/code");

        if !is_vscode || is_helper {
            continue;
        }

        let dir = extract_user_data_dir(process.cmd());
        entries.push((pid_u32, dir));
    }

    #[cfg(target_os = "macos")]
    {
        let output = Command::new("ps").args(["-axo", "pid,command"]).output();
        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines().skip(1) {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let mut parts = line.splitn(2, |ch: char| ch.is_whitespace());
                let pid_str = parts.next().unwrap_or("").trim();
                let cmdline = parts.next().unwrap_or("").trim();
                let pid = match pid_str.parse::<u32>() {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let lower = cmdline.to_lowercase();
                if !lower.contains("visual studio code.app/contents/macos/") {
                    continue;
                }
                if lower.contains("crashpad_handler") || is_helper_command_line(&lower) {
                    continue;
                }
                let dir = extract_user_data_dir_from_command_line(cmdline);
                entries.push((pid, dir));
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let output = powershell_output(&[
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Process -Filter \"Name='Code.exe'\" | ForEach-Object { \"$($_.ProcessId)|$($_.CommandLine)\" }",
        ]);
        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let mut parts = line.splitn(2, '|');
                let pid_str = parts.next().unwrap_or("").trim();
                let cmdline = parts.next().unwrap_or("").trim();
                let pid = match pid_str.parse::<u32>() {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let lower = cmdline.to_lowercase();
                if is_helper_command_line(&lower) {
                    continue;
                }
                let dir = extract_user_data_dir_from_command_line(cmdline);
                entries.push((pid, dir));
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(proc_entries) = std::fs::read_dir("/proc") {
            for entry in proc_entries.flatten() {
                let file_name = entry.file_name();
                let pid_str = file_name.to_string_lossy();
                if !pid_str.chars().all(|ch| ch.is_ascii_digit()) {
                    continue;
                }
                let pid = match pid_str.parse::<u32>() {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let cmdline_path = format!("/proc/{}/cmdline", pid);
                let cmdline = match std::fs::read(&cmdline_path) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                if cmdline.is_empty() {
                    continue;
                }
                let cmdline_str = String::from_utf8_lossy(&cmdline).replace('\0', " ");
                let cmd_lower = cmdline_str.to_lowercase();
                let exe_path = std::fs::read_link(format!("/proc/{}/exe", pid))
                    .ok()
                    .and_then(|p| p.to_str().map(|s| s.to_lowercase()))
                    .unwrap_or_default();
                if !cmd_lower.contains("code") && !exe_path.contains("/code") {
                    continue;
                }
                if is_helper_command_line(&cmd_lower) {
                    continue;
                }
                let dir = extract_user_data_dir_from_command_line(&cmdline_str);
                entries.push((pid, dir));
            }
        }
    }

    let mut map: HashMap<u32, Option<String>> = HashMap::new();
    for (pid, dir) in entries {
        let normalized = dir.and_then(|value| {
            let value = value.trim().to_string();
            if value.is_empty() {
                return None;
            }
            let normalized = normalize_path_for_compare(&value);
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        });
        match map.get(&pid) {
            None => {
                map.insert(pid, normalized);
            }
            Some(existing) => {
                if existing.is_none() && normalized.is_some() {
                    map.insert(pid, normalized);
                }
            }
        }
    }

    let mut result: Vec<(u32, Option<String>)> = map.into_iter().collect();
    result.sort_by_key(|(pid, _)| *pid);
    result
}

pub fn resolve_vscode_pid_from_entries(
    last_pid: Option<u32>,
    user_data_dir: &str,
    entries: &[(u32, Option<String>)],
) -> Option<u32> {
    let target = normalize_path_for_compare(user_data_dir);
    if target.is_empty() {
        return None;
    }

    if let Some(pid) = last_pid {
        if is_pid_running(pid)
            && (entries.iter().any(|(entry_pid, dir)| {
                *entry_pid == pid && dir.as_ref().map(|value| value == &target).unwrap_or(false)
            }) || is_vscode_pid_for_user_data_dir(pid, &target))
        {
            return Some(pid);
        }
    }

    let mut matches = Vec::new();
    for (pid, dir) in entries {
        if let Some(dir) = dir {
            if dir == &target {
                matches.push(*pid);
            }
        } else if is_vscode_pid_for_user_data_dir(*pid, &target) {
            matches.push(*pid);
        }
    }

    pick_preferred_pid(matches)
}

pub fn resolve_vscode_pid(last_pid: Option<u32>, user_data_dir: &str) -> Option<u32> {
    let entries = collect_vscode_process_entries();
    resolve_vscode_pid_from_entries(last_pid, user_data_dir, &entries)
}

fn is_vscode_pid_for_user_data_dir(pid: u32, target: &str) -> bool {
    if pid == 0 || target.is_empty() || !is_pid_running(pid) {
        return false;
    }

    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let process = match system.process(Pid::from_u32(pid)) {
        Some(value) => value,
        None => return false,
    };

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    let name = process.name().to_string_lossy().to_lowercase();
    let exe_path = process
        .exe()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_lowercase();
    let args_str = process
        .cmd()
        .iter()
        .map(|arg| arg.to_string_lossy().to_lowercase())
        .collect::<Vec<String>>()
        .join(" ");
    let is_helper = is_helper_command_line(&args_str) || args_str.contains("crashpad");

    #[cfg(target_os = "macos")]
    let is_vscode = exe_path.contains("visual studio code.app/contents/macos/");
    #[cfg(target_os = "windows")]
    let is_vscode = name == "code.exe" || exe_path.ends_with("\\code.exe");
    #[cfg(target_os = "linux")]
    let is_vscode = name == "code" || exe_path.ends_with("/code");

    if !is_vscode || is_helper {
        return false;
    }

    let args = process.cmd();
    let dir = extract_user_data_dir(args)
        .map(|value| normalize_path_for_compare(&value))
        .filter(|value| !value.is_empty());
    if let Some(value) = dir {
        return value == target;
    }

    is_default_vscode_user_data_dir(target)
}

fn is_default_vscode_user_data_dir(target: &str) -> bool {
    if target.is_empty() {
        return false;
    }

    let default_dir = get_default_vscode_user_data_dir_for_os()
        .map(|value| normalize_path_for_compare(&value))
        .filter(|value| !value.is_empty());

    match default_dir {
        Some(default_dir) => default_dir == target,
        None => false,
    }
}

fn get_default_vscode_user_data_dir_for_os() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir()?;
        return Some(
            home.join("Library")
                .join("Application Support")
                .join("Code")
                .to_string_lossy()
                .to_string(),
        );
    }

    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").ok()?;
        return Some(Path::new(&appdata).join("Code").to_string_lossy().to_string());
    }

    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir()?;
        return Some(home.join(".config").join("Code").to_string_lossy().to_string());
    }

    #[allow(unreachable_code)]
    None
}

pub fn focus_vscode_instance(last_pid: Option<u32>, user_data_dir: &str) -> Result<u32, String> {
    let resolve_start = Instant::now();
    let pid =
        resolve_vscode_pid(last_pid, user_data_dir).ok_or_else(|| "实例未运行，无法定位窗口".to_string())?;
    crate::modules::logger::log_info(&format!(
        "[Focus] VS Code resolve pid={} elapsed={}ms",
        pid,
        resolve_start.elapsed().as_millis()
    ));
    let focus_start = Instant::now();
    focus_window_by_pid(pid)?;
    crate::modules::logger::log_info(&format!(
        "[Focus] VS Code focus pid={} elapsed={}ms",
        pid,
        focus_start.elapsed().as_millis()
    ));
    Ok(pid)
}

#[cfg(target_os = "macos")]

#[allow(dead_code)]
fn list_user_data_dirs_from_ps() -> Vec<String> {
    let mut result = Vec::new();
    let output = Command::new("ps").args(["-axo", "pid,command"]).output();
    let output = match output {
        Ok(value) => value,
        Err(_) => return result,
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_lowercase();
        if !lower.contains("antigravity.app/contents/") {
            continue;
        }
        if lower.contains("antigravity tools.app/contents/") {
            continue;
        }
        if lower.contains("crashpad_handler") {
            continue;
        }
        if let Some(dir) = extract_user_data_dir_from_command_line(line) {
            let normalized = normalize_path_for_compare(&dir);
            if !normalized.is_empty() {
                result.push(normalized);
            }
        }
    }
    result
}

#[cfg(target_os = "macos")]

#[allow(dead_code)]
fn collect_antigravity_process_entries_macos() -> Vec<(u32, Option<String>)> {
    let mut pids = Vec::new();
    if let Ok(output) = Command::new("pgrep")
        .args(["-f", ANTIGRAVITY_APP_PATH])
        .output()
    {
        if output.status.success() {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    pids.push(pid);
                }
            }
        }
    }

    if pids.is_empty() {
        return Vec::new();
    }

    pids.sort();
    pids.dedup();

    let mut result = Vec::new();
    for pid in pids {
        let output = Command::new("ps")
            .args(["-Eww", "-p", &pid.to_string(), "-o", "command="])
            .output();
        let output = match output {
            Ok(value) => value,
            Err(_) => continue,
        };
        if !output.status.success() {
            continue;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let cmdline = line.trim();
            if cmdline.is_empty() {
                continue;
            }
            if !cmdline
                .to_lowercase()
                .contains("antigravity.app/contents/macos/electron")
            {
                continue;
            }
            let dir = extract_user_data_dir_from_command_line(cmdline);
            result.push((pid, dir));
        }
    }

    result
}

#[cfg(target_os = "windows")]
fn list_user_data_dirs_from_powershell() -> Vec<String> {
    let mut result = Vec::new();
    let output = powershell_output(&[
        "-NoProfile",
        "-Command",
        "Get-CimInstance Win32_Process -Filter \"Name='Antigravity.exe'\" | Select-Object -Expand CommandLine",
    ]);
    let output = match output {
        Ok(value) => value,
        Err(_) => return result,
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(dir) = extract_user_data_dir_from_command_line(line) {
            let normalized = normalize_path_for_compare(&dir);
            if !normalized.is_empty() {
                result.push(normalized);
            }
        }
    }
    result
}

#[cfg(target_os = "linux")]
fn list_user_data_dirs_from_proc() -> Vec<String> {
    let mut result = Vec::new();
    let entries = match std::fs::read_dir("/proc") {
        Ok(value) => value,
        Err(_) => return result,
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let pid = file_name.to_string_lossy();
        if !pid.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let cmdline_path = format!("/proc/{}/cmdline", pid);
        let cmdline = match std::fs::read(&cmdline_path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if cmdline.is_empty() {
            continue;
        }
        let cmdline_str = String::from_utf8_lossy(&cmdline).replace('\0', " ");
        let cmd_lower = cmdline_str.to_lowercase();
        let exe_path = std::fs::read_link(format!("/proc/{}/exe", pid))
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_lowercase()))
            .unwrap_or_default();
        if !cmd_lower.contains("antigravity") && !exe_path.contains("antigravity") {
            continue;
        }
        if cmd_lower.contains("tools") || exe_path.contains("tools") {
            continue;
        }
        if let Some(dir) = extract_user_data_dir_from_command_line(&cmdline_str) {
            let normalized = normalize_path_for_compare(&dir);
            if !normalized.is_empty() {
                result.push(normalized);
            }
        }
    }
    result
}


#[allow(dead_code)]
fn collect_antigravity_pids_by_user_data_dir(user_data_dir: &str) -> Vec<u32> {
    let target = normalize_path_for_compare(user_data_dir);
    if target.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let current_pid = std::process::id();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        if pid_u32 == current_pid {
            continue;
        }

        #[cfg(target_os = "macos")]
        let _name = process.name().to_string_lossy().to_lowercase();
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let name = process.name().to_string_lossy().to_lowercase();
        let exe_path = process
            .exe()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_lowercase();

        #[cfg(target_os = "macos")]
        let is_antigravity =
            exe_path.contains("antigravity.app") && !exe_path.contains("antigravity tools.app");
        #[cfg(target_os = "windows")]
        let is_antigravity = name == "antigravity.exe" || exe_path.ends_with("\\antigravity.exe");
        #[cfg(target_os = "linux")]
        let is_antigravity = (name.contains("antigravity") || exe_path.contains("/antigravity"))
            && !name.contains("tools")
            && !exe_path.contains("tools");

        if !is_antigravity {
            continue;
        }

        let args = process.cmd();
        if let Some(dir) = extract_user_data_dir(&args) {
            let normalized = normalize_path_for_compare(&dir);
            if normalized == target {
                result.push(pid_u32);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let entries = collect_antigravity_process_entries_macos();
        if !entries.is_empty() {
            for (pid, dir) in entries {
                if let Some(dir) = dir {
                    let normalized = normalize_path_for_compare(&dir);
                    if normalized == target {
                        result.push(pid);
                    }
                }
            }
        } else {
            let output = Command::new("ps").args(["-axo", "pid,command"]).output();
            if let Ok(output) = output {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines().skip(1) {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    let mut parts = line.splitn(2, |ch: char| ch.is_whitespace());
                    let pid_str = parts.next().unwrap_or("").trim();
                    let cmdline = parts.next().unwrap_or("").trim();
                    let pid = match pid_str.parse::<u32>() {
                        Ok(value) => value,
                        Err(_) => continue,
                    };
                    let lower = cmdline.to_lowercase();
                    if !lower.contains("antigravity.app/contents/")
                        || lower.contains("antigravity tools.app/contents/")
                        || lower.contains("crashpad_handler")
                    {
                        continue;
                    }
                    if let Some(dir) = extract_user_data_dir_from_command_line(cmdline) {
                        let normalized = normalize_path_for_compare(&dir);
                        if normalized == target {
                            result.push(pid);
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let output = powershell_output(&[
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Process -Filter \"Name='Antigravity.exe'\" | ForEach-Object { \"$($_.ProcessId)|$($_.CommandLine)\" }",
        ]);
        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let mut parts = line.splitn(2, '|');
                let pid_str = parts.next().unwrap_or("").trim();
                let cmdline = parts.next().unwrap_or("").trim();
                let pid = match pid_str.parse::<u32>() {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                if let Some(dir) = extract_user_data_dir_from_command_line(cmdline) {
                    let normalized = normalize_path_for_compare(&dir);
                    if normalized == target {
                        result.push(pid);
                    }
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let entries = match std::fs::read_dir("/proc") {
            Ok(value) => value,
            Err(_) => return result,
        };
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let pid_str = file_name.to_string_lossy();
            if !pid_str.chars().all(|ch| ch.is_ascii_digit()) {
                continue;
            }
            let pid = match pid_str.parse::<u32>() {
                Ok(value) => value,
                Err(_) => continue,
            };
            let cmdline_path = format!("/proc/{}/cmdline", pid);
            let cmdline = match std::fs::read(&cmdline_path) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if cmdline.is_empty() {
                continue;
            }
            let cmdline_str = String::from_utf8_lossy(&cmdline).replace('\0', " ");
            let cmd_lower = cmdline_str.to_lowercase();
            let exe_path = std::fs::read_link(format!("/proc/{}/exe", pid))
                .ok()
                .and_then(|p| p.to_str().map(|s| s.to_lowercase()))
                .unwrap_or_default();
            if !cmd_lower.contains("antigravity") && !exe_path.contains("antigravity") {
                continue;
            }
            if cmd_lower.contains("tools") || exe_path.contains("tools") {
                continue;
            }
            if let Some(dir) = extract_user_data_dir_from_command_line(&cmdline_str) {
                let normalized = normalize_path_for_compare(&dir);
                if normalized == target {
                    result.push(pid);
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let entries = match std::fs::read_dir("/proc") {
            Ok(value) => value,
            Err(_) => return result,
        };
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let pid_str = file_name.to_string_lossy();
            if !pid_str.chars().all(|ch| ch.is_ascii_digit()) {
                continue;
            }
            let pid = match pid_str.parse::<u32>() {
                Ok(value) => value,
                Err(_) => continue,
            };
            let cmdline_path = format!("/proc/{}/cmdline", pid);
            let cmdline = match std::fs::read(&cmdline_path) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if cmdline.is_empty() {
                continue;
            }
            let cmdline_str = String::from_utf8_lossy(&cmdline).replace('\0', " ");
            let cmd_lower = cmdline_str.to_lowercase();
            let exe_path = std::fs::read_link(format!("/proc/{}/exe", pid))
                .ok()
                .and_then(|p| p.to_str().map(|s| s.to_lowercase()))
                .unwrap_or_default();
            if !cmd_lower.contains("code") && !exe_path.contains("/code") {
                continue;
            }
            if let Some(dir) = extract_user_data_dir_from_command_line(&cmdline_str) {
                let normalized = normalize_path_for_compare(&dir);
                if normalized == target {
                    result.push(pid);
                }
            }
        }
    }

    result.sort();
    result.dedup();
    result
}

pub fn parse_extra_args(raw: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;

    for ch in raw.chars() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            ' ' | '\t' if !in_single && !in_double => {
                if !current.is_empty() {
                    args.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    args
}

/// 获取正在运行的 Antigravity 实例的 user-data-dir

#[allow(dead_code)]
pub fn list_antigravity_user_data_dirs() -> Vec<String> {
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let current_pid = std::process::id();
    let mut result = Vec::new();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        if pid_u32 == current_pid {
            continue;
        }

        let _name = process.name().to_string_lossy().to_lowercase();
        let exe_path = process
            .exe()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_lowercase();

        let args = process.cmd();

        #[cfg(target_os = "macos")]
        let is_antigravity =
            exe_path.contains("antigravity.app") && !exe_path.contains("antigravity tools.app");
        #[cfg(target_os = "windows")]
        let is_antigravity = _name == "antigravity.exe" || exe_path.ends_with("\\antigravity.exe");
        #[cfg(target_os = "linux")]
        let is_antigravity = (_name.contains("antigravity") || exe_path.contains("/antigravity"))
            && !_name.contains("tools")
            && !exe_path.contains("tools");

        if !is_antigravity {
            continue;
        }

        if let Some(dir) = extract_user_data_dir(&args) {
            let normalized = normalize_path_for_compare(&dir);
            if !normalized.is_empty() {
                result.push(normalized);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let mut pid_dirs: Vec<String> = collect_antigravity_process_entries_macos()
            .into_iter()
            .filter_map(|(_, dir)| dir)
            .map(|dir| normalize_path_for_compare(&dir))
            .filter(|dir| !dir.is_empty())
            .collect();
        if !pid_dirs.is_empty() {
            result.append(&mut pid_dirs);
            result.sort();
            result.dedup();
        } else {
            let mut ps_dirs = list_user_data_dirs_from_ps();
            if !ps_dirs.is_empty() {
                result.append(&mut ps_dirs);
                result.sort();
                result.dedup();
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut ps_dirs = list_user_data_dirs_from_powershell();
        if !ps_dirs.is_empty() {
            result.append(&mut ps_dirs);
            result.sort();
            result.dedup();
        }
    }

    #[cfg(target_os = "linux")]
    {
        let mut proc_dirs = list_user_data_dirs_from_proc();
        if !proc_dirs.is_empty() {
            result.append(&mut proc_dirs);
            result.sort();
            result.dedup();
        }
    }

    result
}

/// 获取所有 Antigravity 进程的 PID（包括主进程和Helper进程）
#[allow(dead_code)]
fn get_antigravity_pids() -> Vec<u32> {
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let mut pids = Vec::new();
    let current_pid = std::process::id();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();

        // 排除自身 PID
        if pid_u32 == current_pid {
            continue;
        }

        let name = process.name().to_string_lossy().to_lowercase();
        let exe_path = process
            .exe()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_lowercase();

        // 通用的辅助进程排除逻辑
        let args = process.cmd();
        let args_str = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_lowercase())
            .collect::<Vec<String>>()
            .join(" ");

        let is_helper = args_str.contains("--type=")
            || name.contains("helper")
            || name.contains("plugin")
            || name.contains("renderer")
            || name.contains("gpu")
            || name.contains("crashpad")
            || name.contains("utility")
            || name.contains("audio")
            || name.contains("sandbox")
            || exe_path.contains("crashpad");

        #[cfg(target_os = "macos")]
        {
            // 匹配 Antigravity 主程序包内的进程，但排除 Helper/Plugin/Renderer 等辅助进程
            if exe_path.contains("antigravity.app") && !is_helper {
                pids.push(pid_u32);
            }
        }

        #[cfg(target_os = "windows")]
        {
            if name == "antigravity.exe" && !is_helper {
                pids.push(pid_u32);
            }
        }

        #[cfg(target_os = "linux")]
        {
            if (name == "antigravity" || exe_path.contains("/antigravity"))
                && !name.contains("tools")
                && !is_helper
            {
                pids.push(pid_u32);
            }
        }
    }

    if !pids.is_empty() {
        crate::modules::logger::log_info(&format!(
            "找到 {} 个 Antigravity 进程: {:?}",
            pids.len(),
            pids
        ));
    }

    pids
}

#[allow(dead_code)]
fn collect_antigravity_main_pids() -> Vec<u32> {
    let entries = collect_antigravity_process_entries();
    if entries.is_empty() {
        return Vec::new();
    }

    let mut groups: HashMap<String, Vec<u32>> = HashMap::new();
    for (pid, dir) in entries {
        let key = dir
            .as_ref()
            .map(|value| normalize_path_for_compare(value))
            .filter(|value| !value.is_empty())
            .unwrap_or_default();
        groups.entry(key).or_default().push(pid);
    }

    let mut result: Vec<u32> = Vec::new();
    for (_, pids) in groups {
        if let Some(pid) = pick_preferred_pid(pids) {
            result.push(pid);
        }
    }
    result.sort();
    result.dedup();
    result
}

/// 关闭 Antigravity 进程
#[allow(dead_code)]
pub fn close_antigravity(timeout_secs: u64) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let _ = timeout_secs; // Silence unused warning on Windows
    crate::modules::logger::log_info("正在关闭 Antigravity...");

    let pids = collect_antigravity_main_pids();
    if pids.is_empty() {
        crate::modules::logger::log_info("Antigravity 未在运行，无需关闭");
        return Ok(());
    }

    crate::modules::logger::log_info(&format!(
        "准备关闭 {} 个 Antigravity 主进程...",
        pids.len()
    ));
    let _ = close_pids(&pids, timeout_secs);

    if is_antigravity_running() {
        return Err("无法关闭 Antigravity 进程，请手动关闭后重试".to_string());
    }

    crate::modules::logger::log_info("Antigravity 已成功关闭");
    Ok(())
}

/// 关闭受管 Antigravity 实例（按 user-data-dir 匹配，包含默认实例目录）
pub fn close_antigravity_instances(user_data_dirs: &[String], timeout_secs: u64) -> Result<(), String> {
    crate::modules::logger::log_info("正在关闭受管 Antigravity 实例...");

    let target_dirs: HashSet<String> = user_data_dirs
        .iter()
        .map(|value| normalize_path_for_compare(value))
        .filter(|value| !value.is_empty())
        .collect();
    if target_dirs.is_empty() {
        crate::modules::logger::log_info("未提供可关闭的 Antigravity 实例目录");
        return Ok(());
    }
    crate::modules::logger::log_info(&format!(
        "[AG Close] target_dirs={:?}, timeout_secs={}",
        target_dirs, timeout_secs
    ));

    let default_dir = crate::modules::instance::get_default_user_data_dir()
        .ok()
        .map(|value| normalize_path_for_compare(&value.to_string_lossy()))
        .filter(|value| !value.is_empty());
    crate::modules::logger::log_info(&format!(
        "[AG Close] default_dir={:?}",
        default_dir
    ));
    let entries = collect_antigravity_process_entries();
    crate::modules::logger::log_info(&format!(
        "[AG Close] collected_entries={:?}",
        entries
    ));
    let mut pids: Vec<u32> = entries
        .iter()
        .filter_map(|(pid, dir)| {
            let resolved_dir = dir
                .as_ref()
                .map(|value| normalize_path_for_compare(value))
                .filter(|value| !value.is_empty())
                .or_else(|| default_dir.clone());
            if resolved_dir
                .as_ref()
                .map(|value| target_dirs.contains(value))
                .unwrap_or(false)
            {
                Some(*pid)
            } else {
                None
            }
        })
        .collect();
    pids.sort();
    pids.dedup();
    if pids.is_empty() {
        crate::modules::logger::log_info("受管 Antigravity 实例未在运行，无需关闭");
        return Ok(());
    }
    crate::modules::logger::log_info(&format!(
        "[AG Close] matched_main_pids={:?}",
        pids
    ));

    crate::modules::logger::log_info(&format!(
        "准备关闭 {} 个受管 Antigravity 主进程...",
        pids.len()
    ));
    if let Err(err) = close_pids(&pids, timeout_secs) {
        crate::modules::logger::log_warn(&format!(
            "[AG Close] close_pids returned error: {}",
            err
        ));
    }

    let mut remaining_entries: Vec<(u32, Option<String>)> = collect_antigravity_process_entries()
        .into_iter()
        .filter(|(_, dir)| {
            let resolved_dir = dir
                .as_ref()
                .map(|value| normalize_path_for_compare(value))
                .filter(|value| !value.is_empty())
                .or_else(|| default_dir.clone());
            resolved_dir
                .as_ref()
                .map(|value| target_dirs.contains(value))
                .unwrap_or(false)
        })
        .collect();
    if !remaining_entries.is_empty() {
        let mut remaining_pids: Vec<u32> = remaining_entries.iter().map(|(pid, _)| *pid).collect();
        remaining_pids.sort();
        remaining_pids.dedup();
        crate::modules::logger::log_warn(&format!(
            "[AG Close] first remaining pids after close={:?}",
            remaining_pids
        ));
        #[cfg(target_os = "windows")]
        {
            log_antigravity_process_details_for_pids(&remaining_pids);
        }

        if !remaining_pids.is_empty() {
            crate::modules::logger::log_warn(&format!(
                "[AG Close] retry force close for remaining pids={:?}",
                remaining_pids
            ));
            if let Err(err) = close_pids(&remaining_pids, 6) {
                crate::modules::logger::log_warn(&format!(
                    "[AG Close] retry close_pids returned error: {}",
                    err
                ));
            }
            remaining_entries = collect_antigravity_process_entries()
                .into_iter()
                .filter(|(_, dir)| {
                    let resolved_dir = dir
                        .as_ref()
                        .map(|value| normalize_path_for_compare(value))
                        .filter(|value| !value.is_empty())
                        .or_else(|| default_dir.clone());
                    resolved_dir
                        .as_ref()
                        .map(|value| target_dirs.contains(value))
                        .unwrap_or(false)
                })
                .collect();
        }
    }
    if !remaining_entries.is_empty() {
        let mut remaining_pids: Vec<u32> = remaining_entries.iter().map(|(pid, _)| *pid).collect();
        remaining_pids.sort();
        remaining_pids.dedup();
        #[cfg(target_os = "windows")]
        {
            log_antigravity_process_details_for_pids(&remaining_pids);
        }
        crate::modules::logger::log_error(&format!(
            "[AG Close] still_running_entries={:?}",
            remaining_entries
        ));
        return Err(format!(
            "无法关闭受管 Antigravity 实例进程，请手动关闭后重试 (remaining_pids={:?})",
            remaining_pids
        ));
    }

    Ok(())
}

/// 关闭指定实例（按 user-data-dir 匹配）

#[allow(dead_code)]
pub fn close_antigravity_instance(user_data_dir: &str, timeout_secs: u64) -> Result<(), String> {
    let target = normalize_path_for_compare(user_data_dir);
    if target.is_empty() {
        return Err("实例目录为空，无法关闭".to_string());
    }

    let pids = collect_antigravity_pids_by_user_data_dir(user_data_dir);
    if pids.is_empty() {
        return Ok(());
    }

    let _ = close_pids(&pids, timeout_secs);

    if !collect_antigravity_pids_by_user_data_dir(user_data_dir).is_empty() {
        return Err("无法关闭实例进程，请手动关闭后重试".to_string());
    }

    Ok(())
}

pub fn close_pid(pid: u32, timeout_secs: u64) -> Result<(), String> {
    if pid == 0 {
        return Err("PID 无效，无法关闭进程".to_string());
    }
    if !is_pid_running(pid) {
        return Ok(());
    }

    send_close_signal(pid);
    if wait_pids_exit(&[pid], timeout_secs) {
        Ok(())
    } else {
        Err("无法关闭实例进程，请手动关闭后重试".to_string())
    }
}

fn send_close_signal(pid: u32) {
    if pid == 0 || !is_pid_running(pid) {
        return;
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        crate::modules::logger::log_info(&format!(
            "[AG Close] taskkill start pid={}",
            pid
        ));
        let output = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output();
        match output {
            Ok(value) => {
                if value.status.success() {
                    crate::modules::logger::log_info(&format!(
                        "[AG Close] taskkill success pid={} status={}",
                        pid, value.status
                    ));
                } else {
                    let stderr = String::from_utf8_lossy(&value.stderr);
                    crate::modules::logger::log_warn(&format!(
                        "[AG Close] taskkill failed pid={} status={} stderr={}",
                        pid,
                        value.status,
                        stderr.trim()
                    ));
                }
            }
            Err(err) => {
                crate::modules::logger::log_warn(&format!(
                    "[AG Close] taskkill error pid={} err={}",
                    pid, err
                ));
            }
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let _ = Command::new("kill").args(["-15", &pid.to_string()]).output();
    }
}

#[cfg(target_os = "windows")]
fn log_antigravity_process_details_for_pids(pids: &[u32]) {
    if pids.is_empty() {
        return;
    }
    let mut unique = pids.to_vec();
    unique.sort();
    unique.dedup();
    let pid_list = unique
        .iter()
        .map(|pid| pid.to_string())
        .collect::<Vec<String>>()
        .join(",");
    let script = format!(
        "$ids=@({}); Get-CimInstance Win32_Process -Filter \"Name='Antigravity.exe'\" | Where-Object {{$ids -contains $_.ProcessId}} | ForEach-Object {{ \"$($_.ProcessId)|$($_.ParentProcessId)|$($_.CommandLine)\" }}",
        pid_list
    );
    match powershell_output(&["-Command", &script]) {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().is_empty() {
                crate::modules::logger::log_warn(&format!(
                    "[AG Close] remaining pid details not found for {:?}",
                    unique
                ));
            } else {
                for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
                    crate::modules::logger::log_warn(&format!(
                        "[AG Close] remaining_pid_detail {}",
                        line.trim()
                    ));
                }
            }
        }
        Err(err) => {
            crate::modules::logger::log_warn(&format!(
                "[AG Close] read remaining pid details failed: {}",
                err
            ));
        }
    }
}

fn wait_pids_exit(pids: &[u32], timeout_secs: u64) -> bool {
    if pids.is_empty() {
        return true;
    }
    let start = std::time::Instant::now();
    loop {
        let mut any_alive = false;
        for pid in pids {
            if *pid != 0 && is_pid_running(*pid) {
                any_alive = true;
                break;
            }
        }
        if !any_alive {
            return true;
        }
        if start.elapsed() >= Duration::from_secs(timeout_secs) {
            return false;
        }
        thread::sleep(Duration::from_millis(350));
    }
}

fn close_pids(pids: &[u32], timeout_secs: u64) -> Result<(), String> {
    if pids.is_empty() {
        return Ok(());
    }
    let mut targets: Vec<u32> = pids
        .iter()
        .copied()
        .filter(|pid| *pid != 0 && is_pid_running(*pid))
        .collect();
    targets.sort();
    targets.dedup();
    if targets.is_empty() {
        return Ok(());
    }
    crate::modules::logger::log_info(&format!(
        "[ClosePids] targets={:?}, timeout_secs={}",
        targets, timeout_secs
    ));

    for pid in &targets {
        send_close_signal(*pid);
    }

    if wait_pids_exit(&targets, timeout_secs) {
        crate::modules::logger::log_info(&format!(
            "[ClosePids] all exited, targets={:?}",
            targets
        ));
        Ok(())
    } else {
        let remaining: Vec<u32> = targets
            .iter()
            .copied()
            .filter(|pid| is_pid_running(*pid))
            .collect();
        crate::modules::logger::log_error(&format!(
            "[ClosePids] timeout, remaining={:?}",
            remaining
        ));
        Err("无法关闭实例进程，请手动关闭后重试".to_string())
    }
}

/// 启动 Antigravity
pub fn start_antigravity() -> Result<u32, String> {
    start_antigravity_with_args("", &[])
}

/// 启动 Antigravity（支持 user-data-dir 与附加参数）
pub fn start_antigravity_with_args(user_data_dir: &str, extra_args: &[String]) -> Result<u32, String> {
    crate::modules::logger::log_info("正在启动 Antigravity...");

    #[cfg(target_os = "macos")]
    let launch_path = resolve_antigravity_launch_path().ok();
    #[cfg(not(target_os = "macos"))]
    let launch_path = resolve_antigravity_launch_path()?;

    #[cfg(target_os = "macos")]
    {
        let app_root = resolve_macos_app_root_from_config("antigravity");
        if let Some(path) = launch_path {
            let mut cmd = Command::new(&path);
            if !user_data_dir.trim().is_empty() {
                cmd.arg("--user-data-dir");
                cmd.arg(user_data_dir.trim());
            }
            cmd.arg("--reuse-window");
            for arg in extra_args {
                if !arg.trim().is_empty() {
                    cmd.arg(arg);
                }
            }
            match spawn_detached_unix(&mut cmd) {
                Ok(child) => {
                    crate::modules::logger::log_info("Antigravity 启动命令已发送");
                    return Ok(child.id());
                }
                Err(e) => {
                    if let Some(app_root) = app_root {
                        let mut args: Vec<String> = Vec::new();
                        if !user_data_dir.trim().is_empty() {
                            args.push("--user-data-dir".to_string());
                            args.push(user_data_dir.trim().to_string());
                        }
                        args.push("--reuse-window".to_string());
                        for arg in extra_args {
                            if !arg.trim().is_empty() {
                                args.push(arg.to_string());
                            }
                        }
                        let pid = spawn_open_app(&app_root, &args)
                            .map_err(|open_err| format!("启动 Antigravity 失败: {}", open_err))?;
                        crate::modules::logger::log_info("Antigravity 启动命令已发送");
                        return Ok(pid);
                    }
                    return Err(format!("启动 Antigravity 失败: {}", e));
                }
            }
        }
        if let Some(app_root) = app_root {
            let mut args: Vec<String> = Vec::new();
            if !user_data_dir.trim().is_empty() {
                args.push("--user-data-dir".to_string());
                args.push(user_data_dir.trim().to_string());
            }
            args.push("--reuse-window".to_string());
            for arg in extra_args {
                if !arg.trim().is_empty() {
                    args.push(arg.to_string());
                }
            }
            let pid = spawn_open_app(&app_root, &args)
                .map_err(|e| format!("启动 Antigravity 失败: {}", e))?;
            crate::modules::logger::log_info("Antigravity 启动命令已发送");
            return Ok(pid);
        }
        return Err(app_path_missing_error("antigravity"));
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        let mut cmd = Command::new(&launch_path);
        if should_detach_child() {
            cmd.creation_flags(0x08000000 | CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS); // CREATE_NO_WINDOW | detached
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
        } else {
            cmd.creation_flags(0x08000000);
        }
        if !user_data_dir.trim().is_empty() {
            cmd.arg("--user-data-dir");
            cmd.arg(user_data_dir.trim());
        }
        cmd.arg("--reuse-window");
        for arg in extra_args {
            if !arg.trim().is_empty() {
                cmd.arg(arg);
            }
        }
        let child = cmd
            .spawn()
            .map_err(|e| format!("启动 Antigravity 失败: {}", e))?;
        crate::modules::logger::log_info(&format!(
            "Antigravity 已启动: {}",
            launch_path.to_string_lossy()
        ));
        return Ok(child.id());
    }

    #[cfg(target_os = "linux")]
    {
        let mut cmd = Command::new(&launch_path);
        if should_detach_child() {
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
        }
        if !user_data_dir.trim().is_empty() {
            cmd.arg("--user-data-dir");
            cmd.arg(user_data_dir.trim());
        }
        cmd.arg("--reuse-window");
        for arg in extra_args {
            if !arg.trim().is_empty() {
                cmd.arg(arg);
            }
        }
        let child = spawn_detached_unix(&mut cmd)
            .map_err(|e| format!("启动 Antigravity 失败: {}", e))?;
        crate::modules::logger::log_info(&format!(
            "Antigravity 已启动: {}",
            launch_path.to_string_lossy()
        ));
        return Ok(child.id());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    Err("不支持的操作系统".to_string())
}

#[cfg(target_os = "macos")]
pub fn collect_codex_process_entries() -> Vec<(u32, Option<String>)> {
    let mut result = Vec::new();
    let mut pids: Vec<u32> = Vec::new();
    if let Ok(output) = Command::new("pgrep")
        .args(["-f", "Codex.app/Contents/MacOS/Codex"])
        .output()
    {
        if output.status.success() {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    pids.push(pid);
                }
            }
        }
    }

    if pids.is_empty() {
        let output = Command::new("ps")
            .args(["-Eww", "-o", "pid=,command="])
            .output();
        let output = match output {
            Ok(value) => value,
            Err(_) => return result,
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.splitn(2, |ch: char| ch.is_whitespace());
            let pid_str = parts.next().unwrap_or("").trim();
            let cmdline = parts.next().unwrap_or("").trim();
            let pid = match pid_str.parse::<u32>() {
                Ok(value) => value,
                Err(_) => continue,
            };
            if !cmdline.to_lowercase().contains("codex.app/contents/macos/codex") {
                continue;
            }
            pids.push(pid);
        }
    }

    pids.sort();
    pids.dedup();

    for pid in pids {
        let output = Command::new("ps")
            .args(["-Eww", "-p", &pid.to_string(), "-o", "command="])
            .output();
        let output = match output {
            Ok(value) => value,
            Err(_) => continue,
        };
        if !output.status.success() {
            continue;
        }
        let cmdline = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if cmdline.is_empty() {
            continue;
        }
        let lower = cmdline.to_lowercase();
        if !lower.contains("codex.app/contents/macos/codex") {
            continue;
        }
        let tokens = split_command_tokens(&cmdline);
        let mut args: Vec<String> = Vec::new();
        let mut env_tokens: Vec<String> = Vec::new();
        let mut saw_env = false;
        for (idx, token) in tokens.into_iter().enumerate() {
            if idx == 0 {
                args.push(token);
                continue;
            }
            if !saw_env && is_env_token(&token) {
                saw_env = true;
                env_tokens.push(token);
                continue;
            }
            if saw_env {
                env_tokens.push(token);
            } else {
                args.push(token);
            }
        }
        let args_lower = args.join(" ").to_lowercase();
        let is_helper = args_lower.contains("--type=")
            || args_lower.contains("helper")
            || args_lower.contains("renderer")
            || args_lower.contains("gpu")
            || args_lower.contains("crashpad")
            || args_lower.contains("utility")
            || args_lower.contains("audio")
            || args_lower.contains("sandbox");
        if is_helper {
            continue;
        }
        let mut codex_home = extract_env_value_from_tokens(&env_tokens, "CODEX_HOME");
        if codex_home.is_none() {
            codex_home = env_tokens
                .iter()
                .find_map(|token| token.strip_prefix("CODEX_HOME="))
                .map(|value| value.to_string());
        }
        if codex_home.is_none() {
            codex_home = extract_env_value(&cmdline, "CODEX_HOME");
        }
        if let Some(ref home) = codex_home {
            crate::modules::logger::log_info(&format!(
                "[Codex Instances] pid={} CODEX_HOME={}",
                pid, home
            ));
        } else {
            crate::modules::logger::log_info(&format!(
                "[Codex Instances] pid={} CODEX_HOME not found",
                pid
            ));
        }
        result.push((pid, codex_home));
    }
    result
}

#[cfg(not(target_os = "macos"))]
pub fn collect_codex_process_entries() -> Vec<(u32, Option<String>)> {
    Vec::new()
}

#[cfg(target_os = "macos")]

#[allow(dead_code)]
fn collect_codex_pids_by_home(target_home: &str, default_home: &str) -> Vec<u32> {
    let target = normalize_path_for_compare(target_home);
    if target.is_empty() {
        return Vec::new();
    }
    let default_normalized = normalize_path_for_compare(default_home);
    let mut result = Vec::new();
    for (pid, home) in collect_codex_process_entries() {
        let resolved = home
            .as_ref()
            .map(|value| normalize_path_for_compare(value))
            .unwrap_or_else(|| default_normalized.clone());
        if resolved == target {
            result.push(pid);
        }
    }
    result.sort();
    result.dedup();
    result
}

/// 获取正在运行的 Codex 实例的 CODEX_HOME

#[allow(dead_code)]
pub fn list_codex_home_dirs(default_home: &str) -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        let mut result = Vec::new();
        let mut has_default = false;
        for (_, home) in collect_codex_process_entries() {
            if let Some(value) = home {
                let normalized = normalize_path_for_compare(&value);
                if !normalized.is_empty() {
                    result.push(normalized);
                }
            } else {
                has_default = true;
            }
        }
        if has_default {
            let normalized = normalize_path_for_compare(default_home);
            if !normalized.is_empty() {
                result.push(normalized);
            }
        }
        result.sort();
        result.dedup();
        return result;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = default_home;
        Vec::new()
    }
}

/// 判断 Codex 是否在运行（仅 macOS）
#[cfg(target_os = "macos")]
pub fn is_codex_running() -> bool {
    #[cfg(target_os = "macos")]
    {
        !collect_codex_process_entries().is_empty()
    }

    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// 启动 Codex（支持 CODEX_HOME 与附加参数，仅 macOS）
pub fn start_codex_with_args(codex_home: &str, extra_args: &[String]) -> Result<u32, String> {
    #[cfg(target_os = "macos")]
    {
        let app_root = resolve_macos_app_root_from_config("codex");
        let launch_path = resolve_codex_launch_path().ok();
        if let Some(path) = launch_path {
            let mut cmd = Command::new(&path);
            if !codex_home.trim().is_empty() {
                cmd.env("CODEX_HOME", codex_home.trim());
            }
            for arg in extra_args {
                if !arg.trim().is_empty() {
                    cmd.arg(arg);
                }
            }
            match spawn_detached_unix(&mut cmd) {
                Ok(child) => {
                    crate::modules::logger::log_info("Codex 启动命令已发送");
                    return Ok(child.id());
                }
                Err(e) => {
                    if codex_home.trim().is_empty() {
                        if let Some(app_root) = app_root {
                            let mut args: Vec<String> = Vec::new();
                            for arg in extra_args {
                                if !arg.trim().is_empty() {
                                    args.push(arg.to_string());
                                }
                            }
                            let pid = spawn_open_app(&app_root, &args)
                                .map_err(|open_err| format!("启动 Codex 失败: {}", open_err))?;
                            crate::modules::logger::log_info("Codex 启动命令已发送");
                            return Ok(pid);
                        }
                    }
                    return Err(format!("启动 Codex 失败: {}", e));
                }
            }
        }
        if codex_home.trim().is_empty() {
            if let Some(app_root) = app_root {
                let mut args: Vec<String> = Vec::new();
                for arg in extra_args {
                    if !arg.trim().is_empty() {
                        args.push(arg.to_string());
                    }
                }
                let pid = spawn_open_app(&app_root, &args)
                    .map_err(|e| format!("启动 Codex 失败: {}", e))?;
                crate::modules::logger::log_info("Codex 启动命令已发送");
                return Ok(pid);
            }
        }
        return Err(app_path_missing_error("codex"));
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (codex_home, extra_args);
        Err("Codex 多开实例仅支持 macOS".to_string())
    }
}

/// 启动 Codex 默认实例（不注入 CODEX_HOME/额外参数，仅 macOS）
pub fn start_codex_default() -> Result<u32, String> {
    #[cfg(target_os = "macos")]
    {
        let app_root = resolve_macos_app_root_from_config("codex");
        if let Ok(launch_path) = resolve_codex_launch_path() {
            let mut cmd = Command::new(&launch_path);
            match spawn_detached_unix(&mut cmd) {
                Ok(child) => {
                    crate::modules::logger::log_info("Codex 启动命令已发送");
                    return Ok(child.id());
                }
                Err(e) => {
                    if let Some(app_root) = app_root {
                        let pid = spawn_open_app(&app_root, &[])
                            .map_err(|open_err| format!("启动 Codex 失败: {}", open_err))?;
                        crate::modules::logger::log_info("Codex 启动命令已发送");
                        return Ok(pid);
                    }
                    return Err(format!("启动 Codex 失败: {}", e));
                }
            }
        }
        if let Some(app_root) = app_root {
            let pid = spawn_open_app(&app_root, &[])
                .map_err(|e| format!("启动 Codex 失败: {}", e))?;
            crate::modules::logger::log_info("Codex 启动命令已发送");
            return Ok(pid);
        }
        return Err(app_path_missing_error("codex"));
    }

    #[cfg(not(target_os = "macos"))]
    Err("Codex 多开实例仅支持 macOS".to_string())
}

/// 关闭 Codex 进程（仅 macOS）
#[allow(dead_code)]
pub fn close_codex(timeout_secs: u64) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        crate::modules::logger::log_info("正在关闭 Codex...");
        let pids: Vec<u32> = collect_codex_process_entries().into_iter().map(|(pid, _)| pid).collect();
        if pids.is_empty() {
            return Ok(());
        }

        let _ = close_pids(&pids, timeout_secs);

        if !collect_codex_process_entries().is_empty() {
            return Err("无法关闭 Codex 进程，请手动关闭后重试".to_string());
        }
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = timeout_secs;
        Err("Codex 多开实例仅支持 macOS".to_string())
    }
}

/// 关闭受管 Codex 实例（按 CODEX_HOME 匹配，包含默认实例目录）
pub fn close_codex_instances(codex_homes: &[String], timeout_secs: u64) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        crate::modules::logger::log_info("正在关闭受管 Codex 实例...");

        let target_homes: HashSet<String> = codex_homes
            .iter()
            .map(|value| normalize_path_for_compare(value))
            .filter(|value| !value.is_empty())
            .collect();
        if target_homes.is_empty() {
            crate::modules::logger::log_info("未提供可关闭的 Codex 实例目录");
            return Ok(());
        }

        let default_home = normalize_path_for_compare(
            &crate::modules::codex_account::get_codex_home()
                .to_string_lossy()
                .to_string(),
        );
        let entries = collect_codex_process_entries();
        let mut pids: Vec<u32> = entries
            .iter()
            .filter_map(|(pid, home)| {
                let resolved_home = home
                    .as_ref()
                    .map(|value| normalize_path_for_compare(value))
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| default_home.clone());
                if !resolved_home.is_empty() && target_homes.contains(&resolved_home) {
                    Some(*pid)
                } else {
                    None
                }
            })
            .collect();
        pids.sort();
        pids.dedup();
        if pids.is_empty() {
            crate::modules::logger::log_info("受管 Codex 实例未在运行，无需关闭");
            return Ok(());
        }

        crate::modules::logger::log_info(&format!(
            "准备关闭 {} 个受管 Codex 主进程...",
            pids.len()
        ));
        let _ = close_pids(&pids, timeout_secs);

        let still_running = collect_codex_process_entries()
            .into_iter()
            .any(|(_, home)| {
                let resolved_home = home
                    .as_ref()
                    .map(|value| normalize_path_for_compare(value))
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| default_home.clone());
                !resolved_home.is_empty() && target_homes.contains(&resolved_home)
            });
        if still_running {
            return Err("无法关闭受管 Codex 实例进程，请手动关闭后重试".to_string());
        }
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (codex_homes, timeout_secs);
        Err("Codex 多开实例仅支持 macOS".to_string())
    }
}

/// 关闭指定 Codex 实例（按 CODEX_HOME 匹配）

#[allow(dead_code)]
pub fn close_codex_instance(codex_home: &str, timeout_secs: u64) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let default_home = crate::modules::codex_account::get_codex_home()
            .to_string_lossy()
            .to_string();
        let target = normalize_path_for_compare(codex_home);
        if target.is_empty() {
            return Err("实例目录为空，无法关闭".to_string());
        }

        let pids = collect_codex_pids_by_home(codex_home, &default_home);
        if pids.is_empty() {
            return Ok(());
        }

        for pid in &pids {
            let _ = close_pid(*pid, timeout_secs);
        }

        if !collect_codex_pids_by_home(codex_home, &default_home).is_empty() {
            return Err("无法关闭实例进程，请手动关闭后重试".to_string());
        }
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (codex_home, timeout_secs);
        Err("Codex 多开实例仅支持 macOS".to_string())
    }
}

/// 检查 OpenCode（桌面端）是否在运行
pub fn is_opencode_running() -> bool {
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let current_pid = std::process::id();
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let app_lower = OPENCODE_APP_NAME.to_lowercase();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        if pid_u32 == current_pid {
            continue;
        }

        let name = process.name().to_string_lossy().to_lowercase();
        let exe_path = process
            .exe()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_lowercase();

        let args = process.cmd();
        let args_str = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_lowercase())
            .collect::<Vec<String>>()
            .join(" ");

        let is_helper = args_str.contains("--type=")
            || name.contains("helper")
            || name.contains("plugin")
            || name.contains("renderer")
            || name.contains("gpu")
            || name.contains("crashpad")
            || name.contains("utility")
            || name.contains("audio")
            || name.contains("sandbox")
            || exe_path.contains("crashpad");

        #[cfg(target_os = "macos")]
        {
            let bundle_lower = format!("{}.app", app_lower);
            if exe_path.contains(&bundle_lower) && !is_helper {
                return true;
            }
        }

        #[cfg(target_os = "windows")]
        {
            if (name == "opencode.exe"
                || name == "opencode"
                || name == app_lower
                || exe_path.contains("opencode"))
                && !is_helper
            {
                return true;
            }
        }

        #[cfg(target_os = "linux")]
        {
            if (name.contains("opencode") || exe_path.contains("/opencode")) && !is_helper {
                return true;
            }
        }
    }

    false
}

fn get_opencode_pids() -> Vec<u32> {
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let mut pids = Vec::new();
    let current_pid = std::process::id();
    #[cfg(target_os = "macos")]
    let app_lower = OPENCODE_APP_NAME.to_lowercase();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        if pid_u32 == current_pid {
            continue;
        }

        let name = process.name().to_string_lossy().to_lowercase();
        let exe_path = process
            .exe()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_lowercase();

        let args = process.cmd();
        let args_str = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_lowercase())
            .collect::<Vec<String>>()
            .join(" ");

        let is_helper = args_str.contains("--type=")
            || name.contains("helper")
            || name.contains("plugin")
            || name.contains("renderer")
            || name.contains("gpu")
            || name.contains("crashpad")
            || name.contains("utility")
            || name.contains("audio")
            || name.contains("sandbox")
            || exe_path.contains("crashpad");

        #[cfg(target_os = "macos")]
        {
            let bundle_lower = format!("{}.app", app_lower);
            if exe_path.contains(&bundle_lower) && !is_helper {
                pids.push(pid_u32);
            }
        }

        #[cfg(target_os = "windows")]
        {
            if (name.contains("opencode") || exe_path.contains("opencode")) && !is_helper {
                pids.push(pid_u32);
            }
        }

        #[cfg(target_os = "linux")]
        {
            if (name.contains("opencode") || exe_path.contains("/opencode")) && !is_helper {
                pids.push(pid_u32);
            }
        }
    }

    if !pids.is_empty() {
        crate::modules::logger::log_info(&format!(
            "找到 {} 个 OpenCode 进程: {:?}",
            pids.len(),
            pids
        ));
    }

    pids
}

/// 关闭 OpenCode（桌面端）
pub fn close_opencode(timeout_secs: u64) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let _ = timeout_secs;

    crate::modules::logger::log_info("正在关闭 OpenCode...");
    let pids = get_opencode_pids();
    if pids.is_empty() {
        crate::modules::logger::log_info("OpenCode 未在运行，无需关闭");
        return Ok(());
    }

    crate::modules::logger::log_info(&format!(
        "准备关闭 {} 个 OpenCode 进程...",
        pids.len()
    ));
    let _ = close_pids(&pids, timeout_secs);

    if is_opencode_running() {
        return Err("无法关闭 OpenCode 进程，请手动关闭后重试".to_string());
    }

    crate::modules::logger::log_info("OpenCode 已成功关闭");
    Ok(())
}

/// 启动 OpenCode（桌面端）
pub fn start_opencode_with_path(custom_path: Option<&str>) -> Result<(), String> {
    crate::modules::logger::log_info("正在启动 OpenCode...");

    #[cfg(target_os = "macos")]
    {
        let target = normalize_custom_path(custom_path).unwrap_or_else(|| OPENCODE_APP_NAME.to_string());

        let output = Command::new("open")
            .args(["-a", &target])
            .output()
            .map_err(|e| format!("启动 OpenCode 失败: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("Unable to find application") {
                return Err("未找到 OpenCode 应用，请在设置中配置启动路径".to_string());
            }
            return Err(format!("启动 OpenCode 失败: {}", stderr));
        }
        crate::modules::logger::log_info(&format!("OpenCode 启动命令已发送: {}", target));
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let mut candidates = Vec::new();
        if let Some(custom) = normalize_custom_path(custom_path) {
            candidates.push(custom);
        }

        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            candidates.push(format!("{}/Programs/OpenCode/OpenCode.exe", local_appdata));
        }

        if let Ok(program_files) = std::env::var("PROGRAMFILES") {
            candidates.push(format!("{}/OpenCode/OpenCode.exe", program_files));
        }

        for candidate in candidates {
            if candidate.contains('/') || candidate.contains('\\') {
                if !std::path::Path::new(&candidate).exists() {
                    continue;
                }
            }
            if Command::new(&candidate)
                .creation_flags(0x08000000)
                .spawn()
                .is_ok()
            {
                crate::modules::logger::log_info(&format!("OpenCode 已启动: {}", candidate));
                return Ok(());
            }
        }

        return Err("未找到 OpenCode 可执行文件，请在设置中配置启动路径".to_string());
    }

    #[cfg(target_os = "linux")]
    {
        let mut candidates = Vec::new();
        if let Some(custom) = normalize_custom_path(custom_path) {
            candidates.push(custom);
        }

        candidates.push("/usr/bin/opencode".to_string());
        candidates.push("/opt/opencode/opencode".to_string());
        candidates.push("opencode".to_string());

        for candidate in candidates {
            if candidate.contains('/') {
                if !std::path::Path::new(&candidate).exists() {
                    continue;
                }
            }
            if Command::new(&candidate).spawn().is_ok() {
                crate::modules::logger::log_info(&format!("OpenCode 已启动: {}", candidate));
                return Ok(());
            }
        }

        return Err("未找到 OpenCode 可执行文件，请在设置中配置启动路径".to_string());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    Err("不支持的操作系统".to_string())
}

pub fn find_pids_by_port(port: u16) -> Result<Vec<u32>, String> {
    let current_pid = std::process::id();
    let mut pids = HashSet::new();

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let output = Command::new("lsof")
            .args(["-nP", &format!("-iTCP:{}", port), "-sTCP:LISTEN", "-t"])
            .output()
            .map_err(|e| format!("执行 lsof 失败: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Ok(pid) = line.trim().parse::<u32>() {
                if pid != current_pid {
                    pids.insert(pid);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let output = Command::new("netstat")
            .args(["-ano", "-p", "tcp"])
            .output()
            .map_err(|e| format!("执行 netstat 失败: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let port_suffix = format!(":{}", port);
        for line in stdout.lines() {
            let line = line.trim();
            if !line.starts_with("TCP") {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 5 {
                continue;
            }
            let local = parts[1];
            let state = parts[3];
            let pid_str = parts[4];
            if !state.eq_ignore_ascii_case("LISTENING") {
                continue;
            }
            if !local.ends_with(&port_suffix) {
                continue;
            }
            if let Ok(pid) = pid_str.parse::<u32>() {
                if pid != current_pid {
                    pids.insert(pid);
                }
            }
        }
    }

    Ok(pids.into_iter().collect())
}

pub fn is_port_in_use(port: u16) -> Result<bool, String> {
    Ok(!find_pids_by_port(port)?.is_empty())
}

pub fn kill_port_processes(port: u16) -> Result<usize, String> {
    let pids = find_pids_by_port(port)?;
    if pids.is_empty() {
        return Ok(0);
    }

    let mut failed = Vec::new();

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        for pid in &pids {
            let output = Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .creation_flags(0x08000000)
                .output();
            match output {
                Ok(out) if out.status.success() => {}
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    failed.push(format!("pid {}: {}", pid, stderr.trim()));
                }
                Err(e) => failed.push(format!("pid {}: {}", pid, e)),
            }
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        for pid in &pids {
            let output = Command::new("kill").args(["-9", &pid.to_string()]).output();
            match output {
                Ok(out) if out.status.success() => {}
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    failed.push(format!("pid {}: {}", pid, stderr.trim()));
                }
                Err(e) => failed.push(format!("pid {}: {}", pid, e)),
            }
        }
    }

    if !failed.is_empty() {
        return Err(format!("关闭进程失败: {}", failed.join("; ")));
    }

    Ok(pids.len())
}

#[allow(dead_code)]
fn get_vscode_pids() -> Vec<u32> {
    let mut result = Vec::new();
    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let current_pid = std::process::id();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        if pid_u32 == current_pid {
            continue;
        }

        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let name = process.name().to_string_lossy().to_lowercase();
        let exe_path = process
            .exe()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_lowercase();

        let args_str = process
            .cmd()
            .iter()
            .map(|arg| arg.to_string_lossy().to_lowercase())
            .collect::<Vec<String>>()
            .join(" ");
        let is_helper = is_helper_command_line(&args_str) || args_str.contains("crashpad");

        #[cfg(target_os = "macos")]
        let is_vscode = exe_path.contains("visual studio code.app");
        #[cfg(target_os = "windows")]
        let is_vscode = name == "code.exe" || exe_path.ends_with("\\code.exe");
        #[cfg(target_os = "linux")]
        let is_vscode = name == "code" || exe_path.ends_with("/code");

        if is_vscode && !is_helper {
            result.push(pid_u32);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let output = Command::new("ps").args(["-axo", "pid,command"]).output();
        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines().skip(1) {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let mut parts = line.splitn(2, |ch: char| ch.is_whitespace());
                let pid_str = parts.next().unwrap_or("").trim();
                let cmdline = parts.next().unwrap_or("").trim();
                let pid = match pid_str.parse::<u32>() {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let lower = cmdline.to_lowercase();
                if !lower.contains("visual studio code.app/contents/") {
                    continue;
                }
                if lower.contains("crashpad_handler") || is_helper_command_line(&lower) {
                    continue;
                }
                result.push(pid);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let output = powershell_output(&[
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Process -Filter \"Name='Code.exe'\" | ForEach-Object { \"$($_.ProcessId)|$($_.CommandLine)\" }",
        ]);
        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let mut parts = line.splitn(2, '|');
                let pid_str = parts.next().unwrap_or("").trim();
                let cmdline = parts.next().unwrap_or("").trim();
                let pid = match pid_str.parse::<u32>() {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let lower = cmdline.to_lowercase();
                if is_helper_command_line(&lower) {
                    continue;
                }
                result.push(pid);
            }
        }
    }

    result.sort();
    result.dedup();
    result
}

pub fn start_vscode_with_args_with_new_window(
    user_data_dir: &str,
    extra_args: &[String],
    use_new_window: bool,
) -> Result<u32, String> {
    #[cfg(target_os = "macos")]
    {
        let target = user_data_dir.trim();
        if target.is_empty() {
            return Err("实例目录为空，无法启动".to_string());
        }
        let launch_path = resolve_vscode_launch_path()?;

        let mut cmd = Command::new(&launch_path);
        cmd.arg("--user-data-dir").arg(target);
        if use_new_window {
            cmd.arg("--new-window");
        } else {
            cmd.arg("--reuse-window");
        }
        for arg in extra_args {
            let trimmed = arg.trim();
            if !trimmed.is_empty() {
                cmd.arg(trimmed);
            }
        }

        let child = spawn_detached_unix(&mut cmd).map_err(|e| format!("启动 VS Code 失败: {}", e))?;
        crate::modules::logger::log_info("VS Code 启动命令已发送");
        return Ok(child.id());
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        let target = user_data_dir.trim();
        if target.is_empty() {
            return Err("实例目录为空，无法启动".to_string());
        }
        let launch_path = resolve_vscode_launch_path()?;

        let mut cmd = Command::new(&launch_path);
        if should_detach_child() {
            cmd.creation_flags(0x08000000 | CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
        } else {
            cmd.creation_flags(0x08000000);
        }
        cmd.arg("--user-data-dir").arg(target);
        if use_new_window {
            cmd.arg("--new-window");
        } else {
            cmd.arg("--reuse-window");
        }
        for arg in extra_args {
            let trimmed = arg.trim();
            if !trimmed.is_empty() {
                cmd.arg(trimmed);
            }
        }

        let child = cmd
            .spawn()
            .map_err(|e| format!("启动 VS Code 失败: {}", e))?;
        crate::modules::logger::log_info("VS Code 启动命令已发送");
        return Ok(child.id());
    }

    #[cfg(target_os = "linux")]
    {
        let target = user_data_dir.trim();
        if target.is_empty() {
            return Err("实例目录为空，无法启动".to_string());
        }
        let launch_path = resolve_vscode_launch_path()?;

        let mut cmd = Command::new(&launch_path);
        if should_detach_child() {
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
        }
        cmd.arg("--user-data-dir").arg(target);
        if use_new_window {
            cmd.arg("--new-window");
        } else {
            cmd.arg("--reuse-window");
        }
        for arg in extra_args {
            let trimmed = arg.trim();
            if !trimmed.is_empty() {
                cmd.arg(trimmed);
            }
        }

        let child = spawn_detached_unix(&mut cmd).map_err(|e| format!("启动 VS Code 失败: {}", e))?;
        crate::modules::logger::log_info("VS Code 启动命令已发送");
        return Ok(child.id());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = (user_data_dir, extra_args, use_new_window);
        Err("GitHub Copilot 多开实例仅支持 macOS、Windows 和 Linux".to_string())
    }
}

#[allow(dead_code)]
pub fn start_vscode_with_args(user_data_dir: &str, extra_args: &[String]) -> Result<u32, String> {
    start_vscode_with_args_with_new_window(user_data_dir, extra_args, false)
}

pub fn start_vscode_default_with_args_with_new_window(
    extra_args: &[String],
    use_new_window: bool,
) -> Result<u32, String> {
    #[cfg(target_os = "macos")]
    {
        let launch_path = resolve_vscode_launch_path()?;
        let mut cmd = Command::new(&launch_path);
        if use_new_window {
            cmd.arg("--new-window");
        } else {
            cmd.arg("--reuse-window");
        }
        for arg in extra_args {
            let trimmed = arg.trim();
            if !trimmed.is_empty() {
                cmd.arg(trimmed);
            }
        }
        let child = spawn_detached_unix(&mut cmd).map_err(|e| format!("启动 VS Code 失败: {}", e))?;
        crate::modules::logger::log_info("VS Code 默认实例启动命令已发送");
        return Ok(child.id());
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        let launch_path = resolve_vscode_launch_path()?;
        let mut cmd = Command::new(&launch_path);
        if should_detach_child() {
            cmd.creation_flags(0x08000000 | CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
        } else {
            cmd.creation_flags(0x08000000);
        }
        if use_new_window {
            cmd.arg("--new-window");
        } else {
            cmd.arg("--reuse-window");
        }
        for arg in extra_args {
            let trimmed = arg.trim();
            if !trimmed.is_empty() {
                cmd.arg(trimmed);
            }
        }
        let child = cmd
            .spawn()
            .map_err(|e| format!("启动 VS Code 失败: {}", e))?;
        crate::modules::logger::log_info("VS Code 默认实例启动命令已发送");
        return Ok(child.id());
    }

    #[cfg(target_os = "linux")]
    {
        let launch_path = resolve_vscode_launch_path()?;
        let mut cmd = Command::new(&launch_path);
        if should_detach_child() {
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
        }
        if use_new_window {
            cmd.arg("--new-window");
        } else {
            cmd.arg("--reuse-window");
        }
        for arg in extra_args {
            let trimmed = arg.trim();
            if !trimmed.is_empty() {
                cmd.arg(trimmed);
            }
        }
        let child = spawn_detached_unix(&mut cmd).map_err(|e| format!("启动 VS Code 失败: {}", e))?;
        crate::modules::logger::log_info("VS Code 默认实例启动命令已发送");
        return Ok(child.id());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = (extra_args, use_new_window);
        Err("GitHub Copilot 多开实例仅支持 macOS、Windows 和 Linux".to_string())
    }
}

pub fn close_vscode(user_data_dirs: &[String], timeout_secs: u64) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let _ = timeout_secs;
    crate::modules::logger::log_info("正在关闭 VS Code...");

    let target_dirs: HashSet<String> = user_data_dirs
        .iter()
        .map(|value| normalize_path_for_compare(value))
        .filter(|value| !value.is_empty())
        .collect();
    if target_dirs.is_empty() {
        crate::modules::logger::log_info("未提供可关闭的实例目录");
        return Ok(());
    }

    let entries = collect_vscode_process_entries();
    let mut grouped: HashMap<String, Vec<u32>> = HashMap::new();
    for (pid, dir) in &entries {
        if let Some(dir) = dir {
            if target_dirs.contains(dir) {
                grouped.entry(dir.clone()).or_default().push(*pid);
            }
            continue;
        }
        for target in &target_dirs {
            if is_vscode_pid_for_user_data_dir(*pid, target) {
                grouped.entry(target.clone()).or_default().push(*pid);
                break;
            }
        }
    }
    let mut pids: Vec<u32> = Vec::new();
    for (_, group) in grouped {
        if let Some(pid) = pick_preferred_pid(group) {
            pids.push(pid);
        }
    }
    pids.sort();
    pids.dedup();
    if pids.is_empty() {
        crate::modules::logger::log_info("受管 VS Code 实例未在运行，无需关闭");
        return Ok(());
    }

    crate::modules::logger::log_info(&format!(
        "准备关闭 {} 个 VS Code 主进程...",
        pids.len()
    ));

    for pid in &pids {
        request_vscode_graceful_close(*pid);
    }
    if wait_pids_exit(&pids, 2) {
        return Ok(());
    }

    let _ = close_pids(&pids, timeout_secs);

    let still_running = collect_vscode_process_entries().into_iter().any(|(pid, dir)| {
        if let Some(dir) = dir {
            target_dirs.contains(&dir)
        } else {
            target_dirs
                .iter()
                .any(|target| is_vscode_pid_for_user_data_dir(pid, target))
        }
    });
    if still_running {
        return Err("无法关闭受管 VS Code 实例进程，请手动关闭后重试".to_string());
    }
    Ok(())
}

fn request_vscode_graceful_close(pid: u32) {
    if pid == 0 || !is_pid_running(pid) {
        return;
    }

    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"System Events\" to set frontmost of (first process whose unix id is {}) to true\n\
tell application \"System Events\" to keystroke \"q\" using command down",
            pid
        );
        match Command::new("osascript").args(["-e", &script]).output() {
            Ok(output) => {
                if output.status.success() {
                    crate::modules::logger::log_info(&format!(
                        "[VSCode Close] 已发送优雅退出请求 pid={}",
                        pid
                    ));
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    crate::modules::logger::log_warn(&format!(
                        "[VSCode Close] 优雅退出失败 pid={} err={}",
                        pid,
                        stderr.trim()
                    ));
                }
            }
            Err(e) => {
                crate::modules::logger::log_warn(&format!(
                    "[VSCode Close] 调用 osascript 失败 pid={} err={}",
                    pid, e
                ));
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
    }
}
