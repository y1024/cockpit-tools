use crate::modules::config;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, System, UpdateKind};

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
const WINDOWS_PROCESS_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// On macOS, extract the executable path from a `ps` command line output.
/// Handles paths with spaces in .app bundles (e.g., "Visual Studio Code.app").
#[cfg(target_os = "macos")]
fn extract_macos_exe_from_cmdline(cmdline: &str) -> Option<String> {
    let lower = cmdline.to_lowercase();
    // For .app bundles: find the binary after Contents/MacOS/
    if let Some(contents_pos) = lower.find(".app/contents/macos/") {
        let after_macos = contents_pos + ".app/contents/macos/".len();
        // Binary name goes until next whitespace or end
        let rest = &cmdline[after_macos..];
        let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
        return Some(cmdline[..after_macos + end].to_string());
    }
    // For non-.app executables: first whitespace-delimited token
    cmdline.split_whitespace().next().map(|s| s.to_string())
}

fn strict_process_detect_enabled() -> bool {
    std::env::var("AG_STRICT_PROCESS_DETECT")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn parse_env_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn command_trace_enabled() -> bool {
    if let Ok(value) = std::env::var("COCKPIT_COMMAND_TRACE") {
        if let Some(enabled) = parse_env_bool(&value) {
            return enabled;
        }
    }
    false
}

fn quote_command_part(part: &str) -> String {
    if part.is_empty() {
        return "\"\"".to_string();
    }
    let needs_quote = part.chars().any(|ch| {
        ch.is_whitespace() || matches!(ch, '"' | '\'' | '$' | '`' | '|' | '&' | ';' | '(' | ')')
    });
    if !needs_quote {
        return part.to_string();
    }
    format!("{:?}", part)
}

fn format_command_preview(command: &Command) -> String {
    let program = quote_command_part(command.get_program().to_string_lossy().as_ref());
    let args = command
        .get_args()
        .map(|arg| quote_command_part(arg.to_string_lossy().as_ref()))
        .collect::<Vec<String>>();
    if args.is_empty() {
        program
    } else {
        format!("{} {}", program, args.join(" "))
    }
}

#[cfg(target_os = "windows")]
fn escape_powershell_single_quoted(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(target_os = "windows")]
fn build_windows_path_filtered_process_probe_script(
    process_name: &str,
    expected_exe_path: &str,
) -> String {
    let process = escape_powershell_single_quoted(process_name);
    let expected = escape_powershell_single_quoted(expected_exe_path);
    format!(
        r#"$processName='{process}';
$expectedRaw='{expected}';
function Normalize-ExePath([string]$path) {{
  if ([string]::IsNullOrWhiteSpace($path)) {{ return $null }}
  $value = $path.Trim().Trim('"')
  $value = [Environment]::ExpandEnvironmentVariables($value)
  if ($value.StartsWith('\\?\UNC\', [System.StringComparison]::OrdinalIgnoreCase)) {{
    $value = '\\' + $value.Substring(8)
  }} elseif ($value.StartsWith('\\?\', [System.StringComparison]::OrdinalIgnoreCase)) {{
    $value = $value.Substring(4)
  }}
  $value = $value -replace '/', '\'
  try {{ $value = [System.IO.Path]::GetFullPath($value) }} catch {{}}
  if ($value.StartsWith('\\?\UNC\', [System.StringComparison]::OrdinalIgnoreCase)) {{
    $value = '\\' + $value.Substring(8)
  }} elseif ($value.StartsWith('\\?\', [System.StringComparison]::OrdinalIgnoreCase)) {{
    $value = $value.Substring(4)
  }}
  return $value.ToLowerInvariant()
}}
function Get-ExePathFromCmdLine([string]$cmdline) {{
  if ([string]::IsNullOrWhiteSpace($cmdline)) {{ return $null }}
  $value = $cmdline.Trim()
  if ($value.StartsWith('"')) {{
    $end = $value.IndexOf('"', 1)
    if ($end -gt 1) {{ return $value.Substring(1, $end - 1) }}
  }}
  $exeMatch = [regex]::Match($value, '^[^""]+?\.exe', [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
  if ($exeMatch.Success) {{ return $exeMatch.Value.Trim() }}
  $space = $value.IndexOf(' ')
  if ($space -gt 0) {{ return $value.Substring(0, $space) }}
  return $value
}}
$expected = Normalize-ExePath $expectedRaw
if ([string]::IsNullOrWhiteSpace($expected)) {{ exit 0 }}
Get-CimInstance Win32_Process -Filter ("Name='" + $processName + "'") |
  Where-Object {{
    $exe = Normalize-ExePath $_.ExecutablePath
    if (-not $exe) {{ $exe = Normalize-ExePath (Get-ExePathFromCmdLine $_.CommandLine) }}
    $exe -eq $expected
  }} |
  ForEach-Object {{ "$($_.ProcessId)|$($_.CommandLine)" }}"#
    )
}

#[cfg(target_os = "windows")]
fn truncate_for_trace(text: &str, max_chars: usize) -> String {
    let mut iter = text.chars();
    let mut current = String::new();
    for _ in 0..max_chars {
        let Some(ch) = iter.next() else {
            return text.to_string();
        };
        current.push(ch);
    }
    if iter.next().is_none() {
        text.to_string()
    } else {
        format!("{}...(truncated)", current)
    }
}

#[cfg(target_os = "windows")]
fn output_bytes_for_trace(bytes: &[u8]) -> String {
    let value = String::from_utf8_lossy(bytes);
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "<empty>".to_string()
    } else {
        truncate_for_trace(trimmed, 4000)
    }
}

fn log_command_trace_exec(command_preview: &str) {
    if !command_trace_enabled() {
        return;
    }
    crate::modules::logger::log_info(&format!("[CmdTrace] EXEC {}", command_preview));
}

#[cfg(target_os = "windows")]
fn log_command_trace_result(
    command_preview: &str,
    result: &std::io::Result<std::process::Output>,
    elapsed: Duration,
) {
    if !command_trace_enabled() {
        return;
    }
    match result {
        Ok(output) => {
            crate::modules::logger::log_info(&format!(
                "[CmdTrace] RESULT elapsed={}ms status={} cmd={}",
                elapsed.as_millis(),
                output.status,
                command_preview
            ));
            crate::modules::logger::log_info(&format!(
                "[CmdTrace] STDOUT cmd={} => {}",
                command_preview,
                output_bytes_for_trace(&output.stdout)
            ));
            crate::modules::logger::log_info(&format!(
                "[CmdTrace] STDERR cmd={} => {}",
                command_preview,
                output_bytes_for_trace(&output.stderr)
            ));
        }
        Err(err) => {
            crate::modules::logger::log_warn(&format!(
                "[CmdTrace] ERROR elapsed={}ms cmd={} err={}",
                elapsed.as_millis(),
                command_preview,
                err
            ));
        }
    }
}

fn log_command_trace_spawn_result(
    command_preview: &str,
    result: &std::io::Result<Child>,
    elapsed: Duration,
) {
    if !command_trace_enabled() {
        return;
    }
    match result {
        Ok(child) => crate::modules::logger::log_info(&format!(
            "[CmdTrace] SPAWN elapsed={}ms pid={} cmd={}",
            elapsed.as_millis(),
            child.id(),
            command_preview
        )),
        Err(err) => crate::modules::logger::log_warn(&format!(
            "[CmdTrace] SPAWN_ERROR elapsed={}ms cmd={} err={}",
            elapsed.as_millis(),
            command_preview,
            err
        )),
    }
}

fn spawn_command_with_trace(cmd: &mut Command) -> std::io::Result<Child> {
    let preview = format_command_preview(cmd);
    log_command_trace_exec(&preview);
    let start = Instant::now();
    let result = cmd.spawn();
    log_command_trace_spawn_result(&preview, &result, start.elapsed());
    result
}

#[cfg(target_os = "windows")]
fn build_powershell_command(args: &[&str]) -> Command {
    use std::os::windows::process::CommandExt;

    let mut final_args: Vec<String> = vec![
        "-WindowStyle".to_string(),
        "Hidden".to_string(),
        "-NonInteractive".to_string(),
        "-NoProfile".to_string(),
    ];
    let mut index = 0;
    while index < args.len() {
        let arg = args[index];
        if arg.eq_ignore_ascii_case("-NoProfile") || arg.eq_ignore_ascii_case("-NonInteractive") {
            index += 1;
            continue;
        }
        if arg.eq_ignore_ascii_case("-WindowStyle") {
            index += if index + 1 < args.len() { 2 } else { 1 };
            continue;
        }
        if arg.eq_ignore_ascii_case("-Command") {
            let script = args.get(index + 1).copied().unwrap_or("");
            let wrapped = format!(
                "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; $OutputEncoding=[System.Text.Encoding]::UTF8; {}",
                script
            );
            final_args.push("-Command".to_string());
            final_args.push(wrapped);
            index += if index + 1 < args.len() { 2 } else { 1 };
            continue;
        }
        final_args.push(arg.to_string());
        index += 1;
    }

    let mut command = Command::new("powershell");
    command.creation_flags(CREATE_NO_WINDOW).args(final_args);
    command
}

#[cfg(target_os = "windows")]
fn powershell_output(args: &[&str]) -> std::io::Result<std::process::Output> {
    let mut command = build_powershell_command(args);
    let preview = format_command_preview(&command);
    log_command_trace_exec(&preview);
    let start = Instant::now();
    let result = command.output();
    log_command_trace_result(&preview, &result, start.elapsed());
    result
}

#[cfg(target_os = "windows")]
fn powershell_output_with_timeout(
    args: &[&str],
    timeout: Duration,
) -> std::io::Result<std::process::Output> {
    use std::io::{Error, ErrorKind, Read};

    let mut command = build_powershell_command(args);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let preview = format_command_preview(&command);
    log_command_trace_exec(&preview);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            if command_trace_enabled() {
                crate::modules::logger::log_warn(&format!(
                    "[CmdTrace] SPAWN_ERROR elapsed=0ms cmd={} err={}",
                    preview, err
                ));
            }
            return Err(err);
        }
    };
    let start = Instant::now();

    loop {
        if let Some(status) = child.try_wait()? {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            if let Some(mut out) = child.stdout.take() {
                let _ = out.read_to_end(&mut stdout);
            }
            if let Some(mut err) = child.stderr.take() {
                let _ = err.read_to_end(&mut stderr);
            }
            let result = Ok(std::process::Output {
                status,
                stdout,
                stderr,
            });
            log_command_trace_result(&preview, &result, start.elapsed());
            return result;
        }

        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let result = Err(Error::new(
                ErrorKind::TimedOut,
                format!(
                    "PowerShell 进程探测超时（{}ms）",
                    timeout.as_millis()
                ),
            ));
            log_command_trace_result(&preview, &result, start.elapsed());
            return result;
        }

        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(target_os = "windows")]
fn cmd_output(args: &[&str]) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;

    let mut command = Command::new("cmd");
    command.creation_flags(CREATE_NO_WINDOW).args(args);
    let preview = format_command_preview(&command);
    log_command_trace_exec(&preview);
    let start = Instant::now();
    let result = command.output();
    log_command_trace_result(&preview, &result, start.elapsed());
    result
}

#[cfg(target_os = "windows")]
fn powershell_output_file(script: &str) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut path = std::env::temp_dir();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    path.push(format!("cockpit_ps_{}_{}.ps1", std::process::id(), unique));

    let file_script = format!(
        "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; $OutputEncoding=[System.Text.Encoding]::UTF8; {}\n",
        script
    );
    std::fs::write(&path, file_script)?;

    let mut command = Command::new("powershell");
    command.creation_flags(CREATE_NO_WINDOW).args([
        "-WindowStyle",
        "Hidden",
        "-NonInteractive",
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        &path.to_string_lossy(),
    ]);
    let preview = format_command_preview(&command);
    log_command_trace_exec(&preview);
    let start = Instant::now();
    let output = command.output();
    log_command_trace_result(&preview, &output, start.elapsed());

    let _ = std::fs::remove_file(&path);
    output
}

#[cfg(target_os = "windows")]
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(target_os = "windows")]
fn powershell_array_literal(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| powershell_quote(value))
        .collect::<Vec<String>>()
        .join(",")
}

#[cfg(target_os = "windows")]
fn normalize_windows_candidate_path(raw: &str) -> Option<std::path::PathBuf> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }

    let mut normalized = text.trim_matches('"').trim_matches('\'').trim().to_string();
    let lowered = normalized.to_lowercase();
    if let Some(index) = lowered.find(".exe") {
        normalized.truncate(index + 4);
    }
    let normalized = normalized
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_end_matches(',')
        .trim()
        .to_string();
    if normalized.is_empty() {
        return None;
    }

    let path = std::path::PathBuf::from(normalized);
    if path.exists() && path.is_file() {
        Some(path)
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
fn score_windows_candidate(
    path: &std::path::Path,
    exe_names_lower: &HashSet<String>,
    keywords_lower: &[String],
) -> Option<i32> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    if file_name.is_empty() {
        return None;
    }

    let path_lower = path.to_string_lossy().to_lowercase();
    let has_keyword = keywords_lower
        .iter()
        .any(|keyword| !keyword.is_empty() && path_lower.contains(keyword));

    if exe_names_lower.contains(&file_name) {
        if file_name == "electron.exe" && !has_keyword {
            return None;
        }
        let mut score = if file_name == "electron.exe" { 60 } else { 100 };
        if has_keyword {
            score += 5;
        }
        return Some(score);
    }

    let is_exe = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("exe"))
        .unwrap_or(false);
    if is_exe && has_keyword {
        return Some(50);
    }
    None
}

#[cfg(target_os = "windows")]
fn parse_windows_exec_candidates(
    app_label: &str,
    exe_names: &[&str],
    display_keywords: &[&str],
    output: std::process::Output,
) -> Option<std::path::PathBuf> {
    let exe_names_lower: HashSet<String> =
        exe_names.iter().map(|value| value.to_lowercase()).collect();
    let keywords_lower: Vec<String> = display_keywords
        .iter()
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .collect();

    let mut seen: HashSet<String> = HashSet::new();
    let mut best: Option<(std::path::PathBuf, i32)> = None;
    let mut raw_lines = 0usize;
    let mut scored_candidates = 0usize;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed_line = line.trim();
        if trimmed_line.is_empty() || trimmed_line.starts_with("STAGE:") {
            continue;
        }
        raw_lines += 1;
        let Some(path) = normalize_windows_candidate_path(line) else {
            continue;
        };
        let dedupe_key = path.to_string_lossy().to_lowercase();
        if !seen.insert(dedupe_key) {
            continue;
        }
        let Some(score) = score_windows_candidate(&path, &exe_names_lower, &keywords_lower) else {
            continue;
        };
        scored_candidates += 1;
        match best.as_ref() {
            Some((_, current_score)) if *current_score >= score => {}
            _ => best = Some((path, score)),
        }
    }

    if let Some((path, score)) = best {
        crate::modules::logger::log_info(&format!(
            "[Path Detect] {} auto detect hit: {}, score={}",
            app_label,
            path.to_string_lossy(),
            score
        ));
        return Some(path);
    }

    let local_appdata = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| "<unset>".to_string());
    let program_files = std::env::var("PROGRAMFILES").unwrap_or_else(|_| "<unset>".to_string());
    let program_files_x86 =
        std::env::var("PROGRAMFILES(X86)").unwrap_or_else(|_| "<unset>".to_string());
    crate::modules::logger::log_warn(&format!(
        "[Path Detect] {} Windows multi-source detect miss: raw_lines={}, unique_candidates={}, scored_candidates={}, local_appdata={}, program_files={}, program_files_x86={}",
        app_label,
        raw_lines,
        seen.len(),
        scored_candidates,
        local_appdata,
        program_files,
        program_files_x86
    ));
    None
}

#[cfg(target_os = "windows")]
fn decode_utf16le(bytes: &[u8]) -> String {
    // Skip UTF-16 LE BOM if present
    let bytes = if bytes.starts_with(&[0xFF, 0xFE]) {
        &bytes[2..]
    } else {
        bytes
    };
    let mut words = Vec::with_capacity(bytes.len() / 2);
    let mut iter = bytes.iter().copied();
    while let Some(lo) = iter.next() {
        let hi = iter.next().unwrap_or(0);
        words.push(u16::from_le_bytes([lo, hi]));
    }
    String::from_utf16_lossy(&words)
}

#[cfg(target_os = "windows")]
fn reg_query_value(key: &str, value_name: &str) -> Option<String> {
    let cmd = if value_name == "(Default)" {
        format!("reg query \"{}\" /ve", key)
    } else {
        format!("reg query \"{}\" /v {}", key, value_name)
    };
    let output = cmd_output(&["/u", "/c", &cmd]).ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = decode_utf16le(&output.stdout);
    let value_name_lower = value_name.to_lowercase();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let matches_name = if value_name == "(Default)" {
            trimmed.starts_with("(Default)")
        } else {
            trimmed.to_lowercase().starts_with(&value_name_lower)
        };
        if !matches_name {
            continue;
        }
        if let Some(pos) = trimmed.find("REG_") {
            let after = &trimmed[pos..];
            if let Some(ws_idx) = after.find(char::is_whitespace) {
                let value = after[ws_idx..].trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn detect_vscode_exec_path_by_registry() -> Option<std::path::PathBuf> {
    let exe_names = ["Code.exe", "Code - Insiders.exe"];
    let app_path_roots = [
        "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\App Paths",
        "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\App Paths",
        "HKLM\\Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\App Paths",
    ];
    for root in app_path_roots {
        for exe in exe_names {
            let key = format!("{}\\{}", root, exe);
            if let Some(value) = reg_query_value(&key, "(Default)") {
                if let Some(path) = normalize_windows_candidate_path(&value) {
                    crate::modules::logger::log_info(&format!(
                        "[Path Detect] vscode registry hit: {}",
                        path.to_string_lossy()
                    ));
                    return Some(path);
                }
            }
            if let Some(path_root) = reg_query_value(&key, "Path") {
                let candidate = std::path::PathBuf::from(path_root).join(exe);
                if candidate.exists() {
                    crate::modules::logger::log_info(&format!(
                        "[Path Detect] vscode registry hit: {}",
                        candidate.to_string_lossy()
                    ));
                    return Some(candidate);
                }
            }
        }
    }

    let uninstall_roots = [
        "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        "HKLM\\Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    ];
    let keywords = ["visual studio code", "vs code", "vscode"];
    for root in uninstall_roots {
        let cmd = format!("reg query \"{}\" /s /v DisplayName", root);
        let output = match cmd_output(&["/u", "/c", &cmd]) {
            Ok(o) => o,
            Err(_) => continue,
        };
        if !output.status.success() {
            continue;
        }
        let stdout = decode_utf16le(&output.stdout);
        let mut current_key: Option<String> = None;
        let mut matched_keys: Vec<String> = Vec::new();
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("HKEY_") {
                current_key = Some(trimmed.to_string());
                continue;
            }
            if !trimmed.to_lowercase().starts_with("displayname") {
                continue;
            }
            if let Some(pos) = trimmed.find("REG_") {
                let after = &trimmed[pos..];
                if let Some(ws_idx) = after.find(char::is_whitespace) {
                    let value = after[ws_idx..].trim().to_lowercase();
                    if keywords.iter().any(|kw| value.contains(kw)) {
                        if let Some(key) = current_key.as_ref() {
                            matched_keys.push(key.clone());
                        }
                    }
                }
            }
        }
        for key in matched_keys {
            for value_name in ["DisplayIcon", "UninstallString"] {
                if let Some(value) = reg_query_value(&key, value_name) {
                    if let Some(path) = normalize_windows_candidate_path(&value) {
                        crate::modules::logger::log_info(&format!(
                            "[Path Detect] vscode registry hit: {}",
                            path.to_string_lossy()
                        ));
                        return Some(path);
                    }
                }
            }
            if let Some(install_root) = reg_query_value(&key, "InstallLocation") {
                for exe in exe_names {
                    let candidate = std::path::PathBuf::from(&install_root).join(exe);
                    if candidate.exists() {
                        crate::modules::logger::log_info(&format!(
                            "[Path Detect] vscode registry hit: {}",
                            candidate.to_string_lossy()
                        ));
                        return Some(candidate);
                    }
                }
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
pub fn detect_windows_exec_path_by_signatures(
    app_label: &str,
    exe_names: &[&str],
    command_names: &[&str],
    protocol_names: &[&str],
    display_keywords: &[&str],
) -> Option<std::path::PathBuf> {
    if exe_names.is_empty() {
        return None;
    }

    let exe_array = powershell_array_literal(exe_names);
    let command_array = powershell_array_literal(command_names);
    let protocol_array = powershell_array_literal(protocol_names);
    let keyword_array = powershell_array_literal(display_keywords);

    let script = format!(
        r#"$ErrorActionPreference='SilentlyContinue'
Write-Output 'STAGE:BEGIN'
$exeNames=@({exe_array})
$commandNames=@({command_array})
$protocolNames=@({protocol_array})
$keywords=@({keyword_array})

function Normalize-Candidate([string]$raw) {{
  if ([string]::IsNullOrWhiteSpace($raw)) {{ return $null }}
  $text = $raw.Trim()
  if ($text -match '(?i)(?<p>[A-Za-z]:\\.+?\.exe)') {{
    $text = $matches['p']
  }}
  $text = $text.Trim().Trim('"').Trim("'")
  if ([string]::IsNullOrWhiteSpace($text)) {{ return $null }}
  return $text
}}

function Emit-Candidate([string]$raw) {{
  $candidate = Normalize-Candidate $raw
  if ([string]::IsNullOrWhiteSpace($candidate)) {{ return }}
  if (Test-Path -LiteralPath $candidate) {{ Write-Output $candidate }}
}}

Write-Output 'STAGE:APP_PATHS'
$appPathRoots=@(
  'HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths',
  'HKLM:\Software\Microsoft\Windows\CurrentVersion\App Paths',
  'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths'
)
foreach ($root in $appPathRoots) {{
  foreach ($exe in $exeNames) {{
    $keyPath = Join-Path $root $exe
    $entry = Get-ItemProperty -Path $keyPath -ErrorAction SilentlyContinue
    if ($entry) {{
      Emit-Candidate $entry.'(default)'
      if ($entry.Path) {{
        Emit-Candidate (Join-Path $entry.Path $exe)
      }}
    }}
  }}
}}

Write-Output 'STAGE:UNINSTALL'
$uninstallRoots=@(
  'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
foreach ($root in $uninstallRoots) {{
  Get-ItemProperty -Path $root -ErrorAction SilentlyContinue | ForEach-Object {{
    $display = [string]$_.DisplayName
    $displayLower = $display.ToLowerInvariant()
    $hit = $false
    foreach ($kw in $keywords) {{
      if ([string]::IsNullOrWhiteSpace($kw)) {{ continue }}
      if ($displayLower.Contains($kw.ToLowerInvariant())) {{
        $hit = $true
        break
      }}
    }}
    if (-not $hit) {{ return }}
    Emit-Candidate $_.DisplayIcon
    Emit-Candidate $_.UninstallString
    $install = [string]$_.InstallLocation
    if (-not [string]::IsNullOrWhiteSpace($install)) {{
      foreach ($exe in $exeNames) {{
        Emit-Candidate (Join-Path $install $exe)
      }}
    }}
  }}
}}

Write-Output 'STAGE:CLASSES'
$classRoots=@('HKCU:\Software\Classes','HKLM:\Software\Classes')
foreach ($protocol in $protocolNames) {{
  if ([string]::IsNullOrWhiteSpace($protocol)) {{ continue }}
  foreach ($classRoot in $classRoots) {{
    $commandPath = Join-Path (Join-Path $classRoot $protocol) 'shell\open\command'
    Emit-Candidate ((Get-ItemProperty -Path $commandPath -ErrorAction SilentlyContinue).'(default)')
  }}
}}

Write-Output 'STAGE:SHORTCUTS'
$shortcutRoots=@(
  "$env:ProgramData\Microsoft\Windows\Start Menu\Programs",
  "$env:APPDATA\Microsoft\Windows\Start Menu\Programs",
  "$env:USERPROFILE\Desktop",
  "$env:PUBLIC\Desktop"
)
$shell = $null
try {{ $shell = New-Object -ComObject WScript.Shell }} catch {{}}
if ($shell) {{
  foreach ($root in $shortcutRoots) {{
    if (-not (Test-Path -LiteralPath $root)) {{ continue }}
    Get-ChildItem -Path $root -Filter *.lnk -Recurse -ErrorAction SilentlyContinue | ForEach-Object {{
      try {{
        $shortcut = $shell.CreateShortcut($_.FullName)
        Emit-Candidate $shortcut.TargetPath
      }} catch {{}}
    }}
  }}
}}

Write-Output 'STAGE:COMMANDS'
foreach ($commandName in $commandNames) {{
  if ([string]::IsNullOrWhiteSpace($commandName)) {{ continue }}
  $command = Get-Command $commandName -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($command) {{
    Emit-Candidate $command.Source
    Emit-Candidate $command.Definition
  }}
}}
Write-Output 'STAGE:END'
exit 0
"#
    );

    let output = match powershell_output(&["-Command", &script]) {
        Ok(value) => value,
        Err(err) => {
            crate::modules::logger::log_warn(&format!(
                "[Path Detect] {} PowerShell detect failed: {}",
                app_label, err
            ));
            return None;
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        crate::modules::logger::log_warn(&format!(
            "[Path Detect] {} PowerShell command failed(-Command): status={}, stdout_head={}, stderr_head={}",
            app_label,
            output.status,
            stdout.chars().take(400).collect::<String>(),
            stderr.chars().take(400).collect::<String>()
        ));
        if strict_process_detect_enabled() {
            crate::modules::logger::log_warn(&format!(
                "[Path Detect] {} strict mode enabled, skip -File fallback",
                app_label
            ));
            return None;
        }
        let retry = match powershell_output_file(&script) {
            Ok(value) => value,
            Err(err) => {
                crate::modules::logger::log_warn(&format!(
                    "[Path Detect] {} PowerShell -File detect failed: {}",
                    app_label, err
                ));
                return None;
            }
        };
        if !retry.status.success() {
            let retry_stderr = String::from_utf8_lossy(&retry.stderr);
            let retry_stdout = String::from_utf8_lossy(&retry.stdout);
            crate::modules::logger::log_warn(&format!(
                "[Path Detect] {} PowerShell command failed(-File): status={}, stdout_head={}, stderr_head={}",
                app_label,
                retry.status,
                retry_stdout.chars().take(400).collect::<String>(),
                retry_stderr.chars().take(400).collect::<String>()
            ));
            return None;
        }
        crate::modules::logger::log_info(&format!(
            "[Path Detect] {} PowerShell -File fallback succeeded after -Command failure",
            app_label
        ));
        return parse_windows_exec_candidates(app_label, exe_names, display_keywords, retry);
    }

    parse_windows_exec_candidates(app_label, exe_names, display_keywords, output)
}

#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub fn detect_windows_exec_path_by_signatures(
    _app_label: &str,
    _exe_names: &[&str],
    _command_names: &[&str],
    _protocol_names: &[&str],
    _display_keywords: &[&str],
) -> Option<std::path::PathBuf> {
    None
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

#[cfg(target_os = "macos")]
fn sanitize_macos_gui_launch_env(cmd: &mut Command) {
    // Avoid inheriting Cockpit bundle identity into child GUI apps.
    cmd.env_remove("__CFBundleIdentifier");
    cmd.env_remove("XPC_SERVICE_NAME");
}

#[cfg(not(target_os = "macos"))]
fn sanitize_macos_gui_launch_env(_cmd: &mut Command) {}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn spawn_detached_unix(cmd: &mut Command) -> Result<Child, String> {
    use std::os::unix::process::CommandExt;
    if !should_detach_child() {
        return spawn_command_with_trace(cmd).map_err(|e| format!("启动失败: {}", e));
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
    spawn_command_with_trace(cmd).map_err(|e| format!("启动失败: {}", e))
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
    spawn_open_app_with_options(app_root, args, false)
}

#[cfg(target_os = "macos")]
fn spawn_open_app_with_options(
    app_root: &str,
    args: &[String],
    force_new_instance: bool,
) -> Result<u32, String> {
    let mut cmd = Command::new("open");
    sanitize_macos_gui_launch_env(&mut cmd);
    if force_new_instance {
        cmd.arg("-n");
    }
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
    #[cfg(target_os = "macos")]
    {
        // Use ps to avoid sysinfo TCC dialogs on macOS
        let output = Command::new("ps").args(["-axww", "-o", "pid=,command="]).output().ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            let mut parts = line.splitn(2, |ch: char| ch.is_whitespace());
            let _pid_str = parts.next().unwrap_or("").trim();
            let cmdline = parts.next().unwrap_or("").trim();
            let lower = cmdline.to_lowercase();
            if !lower.contains("antigravity.app/contents/") { continue; }
            if lower.contains("antigravity tools.app/contents/") { continue; }
            if lower.contains("--type=") || lower.contains("crashpad_handler") { continue; }
            if let Some(exe) = extract_macos_exe_from_cmdline(cmdline) {
                return Some(std::path::PathBuf::from(exe));
            }
        }
        return None;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut system = System::new();
        system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet));

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
}

fn find_vscode_process_exe() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        // Use ps to avoid sysinfo TCC dialogs on macOS
        let output = Command::new("ps").args(["-axww", "-o", "pid=,command="]).output().ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            let mut parts = line.splitn(2, |ch: char| ch.is_whitespace());
            let _pid_str = parts.next().unwrap_or("").trim();
            let cmdline = parts.next().unwrap_or("").trim();
            let lower = cmdline.to_lowercase();
            if !lower.contains("visual studio code.app/contents/macos/") { continue; }
            if lower.contains("--type=") || lower.contains("crashpad_handler") { continue; }
            if let Some(exe) = extract_macos_exe_from_cmdline(cmdline) {
                return Some(std::path::PathBuf::from(exe));
            }
        }
        return None;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut system = System::new();
        system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet));

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
}

#[cfg(target_os = "macos")]
fn find_codex_process_exe() -> Option<std::path::PathBuf> {
    // Use ps to avoid sysinfo TCC dialogs on macOS
    let output = Command::new("ps").args(["-axww", "-o", "pid=,command="]).output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let mut parts = line.splitn(2, |ch: char| ch.is_whitespace());
        let _pid_str = parts.next().unwrap_or("").trim();
        let cmdline = parts.next().unwrap_or("").trim();
        let lower = cmdline.to_lowercase();
        if !lower.contains("codex.app/contents/macos/codex") { continue; }
        if lower.contains("--type=") || lower.contains("crashpad_handler") { continue; }
        if let Some(exe) = extract_macos_exe_from_cmdline(cmdline) {
            return Some(std::path::PathBuf::from(exe));
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
        if let Some(path) = detect_windows_exec_path_by_signatures(
            "antigravity",
            &["Antigravity.exe", "Electron.exe"],
            &["antigravity"],
            &["antigravity"],
            &["antigravity"],
        ) {
            return Some(path);
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
        if let Some(path) = detect_windows_exec_path_by_signatures(
            "vscode",
            &["Code.exe", "Code - Insiders.exe"],
            &["code", "code-insiders"],
            &["vscode", "vscode-insiders"],
            &["visual studio code", "vs code", "vscode"],
        ) {
            return Some(path);
        }
        if let Some(path) = detect_vscode_exec_path_by_registry() {
            return Some(path);
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

#[cfg(target_os = "windows")]
fn compare_windows_store_version(left: &[u32], right: &[u32]) -> std::cmp::Ordering {
    let max_len = left.len().max(right.len());
    for idx in 0..max_len {
        let left_part = *left.get(idx).unwrap_or(&0);
        let right_part = *right.get(idx).unwrap_or(&0);
        match left_part.cmp(&right_part) {
            std::cmp::Ordering::Equal => continue,
            non_eq => return non_eq,
        }
    }
    std::cmp::Ordering::Equal
}

#[cfg(target_os = "windows")]
fn parse_codex_store_version_from_dir_name(dir_name: &str) -> Option<Vec<u32>> {
    let lower = dir_name.to_ascii_lowercase();
    if !lower.starts_with("openai.codex_") {
        return None;
    }
    let suffix = dir_name.get("OpenAI.Codex_".len()..)?;
    let version_part = suffix.split('_').next()?.trim();
    if version_part.is_empty() {
        return None;
    }
    let mut version: Vec<u32> = Vec::new();
    for part in version_part.split('.') {
        if part.is_empty() {
            return None;
        }
        version.push(part.parse::<u32>().ok()?);
    }
    if version.is_empty() {
        return None;
    }
    Some(version)
}

#[cfg(target_os = "windows")]
fn detect_codex_exec_path_by_windowsapps_scan() -> Option<std::path::PathBuf> {
    let mut best: Option<(Vec<u32>, std::path::PathBuf)> = None;

    for drive in b'A'..=b'Z' {
        let drive_letter = drive as char;
        let windows_apps_root = if drive_letter == 'C' {
            format!(r"{}:\Program Files\WindowsApps", drive_letter)
        } else {
            format!(r"{}:\WindowsApps", drive_letter)
        };
        let root_path = std::path::PathBuf::from(&windows_apps_root);
        if !root_path.exists() {
            continue;
        }

        let entries = match std::fs::read_dir(&root_path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(value) => value,
                Err(_) => continue,
            };
            if !file_type.is_dir() {
                continue;
            }

            let dir_name = entry.file_name();
            let dir_name = dir_name.to_string_lossy();
            let Some(version) = parse_codex_store_version_from_dir_name(&dir_name) else {
                continue;
            };

            let candidate = entry.path().join("app").join("Codex.exe");
            if !candidate.exists() {
                continue;
            }

            let replace = match &best {
                None => true,
                Some((best_version, _)) => {
                    compare_windows_store_version(&version, best_version).is_gt()
                }
            };
            if replace {
                best = Some((version, candidate));
            }
        }
    }

    if let Some((_, path)) = best {
        crate::modules::logger::log_info(&format!(
            "[Path Detect] codex windowsapps scan hit: {}",
            path.to_string_lossy()
        ));
        return Some(path);
    }

    None
}

#[cfg(target_os = "windows")]
fn detect_codex_exec_path_by_appx_install_location() -> Option<std::path::PathBuf> {
    let script = r#"$pkg = Get-AppxPackage -Name 'OpenAI.Codex' -ErrorAction SilentlyContinue |
  Sort-Object -Property Version -Descending |
  Select-Object -First 1
if ($pkg -and -not [string]::IsNullOrWhiteSpace($pkg.InstallLocation)) {
  Write-Output ([string]$pkg.InstallLocation.Trim())
}"#;

    let output = match powershell_output(&["-Command", script]) {
        Ok(value) => value,
        Err(_) => powershell_output_file(script).ok()?,
    };
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let install_location = line.trim().trim_matches('"');
        if install_location.is_empty() {
            continue;
        }
        let candidate = std::path::PathBuf::from(install_location)
            .join("app")
            .join("Codex.exe");
        if candidate.exists() {
            crate::modules::logger::log_info(&format!(
                "[Path Detect] codex appx install hit: {}",
                candidate.to_string_lossy()
            ));
            return Some(candidate);
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

    #[cfg(target_os = "windows")]
    {
        if let Some(path) = detect_codex_exec_path_by_windowsapps_scan() {
            return Some(path);
        }
        if let Some(path) = detect_codex_exec_path_by_appx_install_location() {
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
        if let Some(path) = detect_windows_exec_path_by_signatures(
            "opencode",
            &["OpenCode.exe", "opencode.exe"],
            &["opencode"],
            &["opencode"],
            &["opencode", "open code"],
        ) {
            return Some(path);
        }
    }

    #[cfg(target_os = "linux")]
    {
        let candidates = ["/usr/bin/opencode", "/opt/opencode/opencode"];
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
    if let Some(custom) =
        normalize_custom_path(Some(&config::get_user_config().antigravity_app_path))
    {
        if let Some(exec) = resolve_macos_exec_path(&custom, "Electron") {
            return Ok(exec);
        }
        return Err(app_path_missing_error("antigravity"));
    }

    Err(app_path_missing_error("antigravity"))
}

pub fn ensure_antigravity_launch_path_configured() -> Result<(), String> {
    resolve_antigravity_launch_path().map(|_| ())
}

pub fn ensure_vscode_launch_path_configured() -> Result<(), String> {
    resolve_vscode_launch_path().map(|_| ())
}

pub fn ensure_codex_launch_path_configured() -> Result<(), String> {
    resolve_codex_launch_path().map(|_| ())
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
        return Err(app_path_missing_error("vscode"));
    }

    Err(app_path_missing_error("vscode"))
}

#[cfg(target_os = "macos")]
fn resolve_codex_launch_path() -> Result<std::path::PathBuf, String> {
    if let Some(custom) = normalize_custom_path(Some(&config::get_user_config().codex_app_path)) {
        if let Some(exec) = resolve_macos_exec_path(&custom, "Codex") {
            return Ok(exec);
        }
        return Err(app_path_missing_error("codex"));
    }

    Err(app_path_missing_error("codex"))
}

#[cfg(not(target_os = "macos"))]
fn resolve_codex_launch_path() -> Result<std::path::PathBuf, String> {
    if let Some(custom) = normalize_custom_path(Some(&config::get_user_config().codex_app_path)) {
        if let Some(exec) = resolve_macos_exec_path(&custom, "Codex") {
            return Ok(exec);
        }
        return Err(app_path_missing_error("codex"));
    }

    Err(app_path_missing_error("codex"))
}

pub fn detect_and_save_app_path(app: &str, force: bool) -> Option<String> {
    let current = config::get_user_config();
    match app {
        "antigravity" => {
            if !force && !current.antigravity_app_path.trim().is_empty() {
                return Some(current.antigravity_app_path);
            }
            if let Some(detected) = detect_antigravity_exec_path() {
                update_app_path_in_config("antigravity", &detected);
                return Some(config::get_user_config().antigravity_app_path);
            }
        }
        "codex" => {
            if !force && !current.codex_app_path.trim().is_empty() {
                return Some(current.codex_app_path);
            }
            if let Some(detected) = detect_codex_exec_path() {
                update_app_path_in_config("codex", &detected);
                return Some(config::get_user_config().codex_app_path);
            }
        }
        "vscode" => {
            if !force && !current.vscode_app_path.trim().is_empty() {
                return Some(current.vscode_app_path);
            }
            if let Some(detected) = detect_vscode_exec_path() {
                update_app_path_in_config("vscode", &detected);
                return Some(config::get_user_config().vscode_app_path);
            }
        }
        "opencode" => {
            if !force && !current.opencode_app_path.trim().is_empty() {
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
    #[cfg(target_os = "macos")]
    {
        // Use ps to avoid sysinfo TCC dialogs on macOS
        if let Ok(output) = Command::new("ps").args(["-axww", "-o", "command="]).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let lower = line.trim().to_lowercase();
                if lower.contains("antigravity.app/contents/")
                    && !lower.contains("antigravity tools.app/contents/")
                    && !lower.contains("--type=")
                    && !lower.contains("crashpad_handler")
                {
                    return true;
                }
            }
        }
        return false;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut system = System::new();
        system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet));

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

            if is_antigravity_main_process(&name, &exe_path, Some(&args_str)) {
                return true;
            }
        }

        false
    }
}

pub fn is_pid_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        // On macOS, use ps to check process existence to avoid sysinfo TCC dialogs.
        return Command::new("ps")
            .args(["-p", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut system = System::new();
        system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet));
        system.process(Pid::from(pid as usize)).is_some()
    }
}

#[allow(dead_code)]
fn extract_user_data_dir(args: &[std::ffi::OsString]) -> Option<String> {
    let tokens: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect();
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
        let end = rest[1..]
            .find('\'')
            .map(|idx| idx + 1)
            .unwrap_or(rest.len());
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
        let end = rest[1..]
            .find('\'')
            .map(|idx| idx + 1)
            .unwrap_or(rest.len());
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

    #[cfg(target_os = "windows")]
    fn normalize_windows_extended_path(raw: &str) -> String {
        let mut value = raw.trim().trim_matches('"').replace('/', "\\");
        let lower = value.to_ascii_lowercase();
        if lower.starts_with("\\\\?\\unc\\") {
            let rest = value
                .chars()
                .skip("\\\\?\\UNC\\".chars().count())
                .collect::<String>();
            value = format!("\\\\{}", rest);
        } else if lower.starts_with("\\\\?\\") {
            value = value
                .chars()
                .skip("\\\\?\\".chars().count())
                .collect::<String>();
        }
        value
    }

    #[cfg(target_os = "windows")]
    let normalized_input = normalize_windows_extended_path(trimmed);
    #[cfg(not(target_os = "windows"))]
    let normalized_input = trimmed.to_string();

    let resolved = std::fs::canonicalize(&normalized_input)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(normalized_input);

    #[cfg(target_os = "windows")]
    let resolved = normalize_windows_extended_path(&resolved);

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
        || cmdline_lower.contains("--node-ipc")
        || cmdline_lower.contains("--clientprocessid=")
        || cmdline_lower.contains("\\resources\\app\\extensions\\")
        || cmdline_lower.contains("/resources/app/extensions/")
}

fn is_antigravity_main_process(
    name: &str,
    exe_path: &str,
    command_line_lower: Option<&str>,
) -> bool {
    let cmdline = command_line_lower.unwrap_or("");
    if cmdline.contains("antigravity tools") || cmdline.contains("antigravity tools.app/contents/")
    {
        return false;
    }
    if !cmdline.is_empty() && is_helper_command_line(cmdline) {
        return false;
    }

    #[cfg(target_os = "macos")]
    {
        let _ = name;
        return exe_path.contains("antigravity.app")
            && !exe_path.contains("antigravity tools.app")
            && !exe_path.contains("crashpad");
    }

    #[cfg(target_os = "windows")]
    {
        return (name == "antigravity.exe" || exe_path.ends_with("\\antigravity.exe"))
            && !exe_path.contains("crashpad");
    }

    #[cfg(target_os = "linux")]
    {
        return (name.contains("antigravity") || exe_path.contains("/antigravity"))
            && !name.contains("tools")
            && !exe_path.contains("tools");
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = (name, exe_path);
        false
    }
}

fn collect_running_process_exe_by_pid() -> HashMap<u32, String> {
    let mut map = HashMap::new();

    #[cfg(target_os = "macos")]
    {
        // Use ps to avoid sysinfo TCC dialogs on macOS
        if let Ok(output) = Command::new("ps").args(["-axww", "-o", "pid=,command="]).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if line.is_empty() { continue; }
                let mut parts = line.splitn(2, |ch: char| ch.is_whitespace());
                let pid_str = parts.next().unwrap_or("").trim();
                let cmdline = parts.next().unwrap_or("").trim();
                let pid = match pid_str.parse::<u32>() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if let Some(exe) = extract_macos_exe_from_cmdline(cmdline) {
                    let normalized = normalize_path_for_compare(&exe);
                    if !normalized.is_empty() {
                        map.insert(pid, normalized);
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut system = System::new();
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
        );
        for (pid, process) in system.processes() {
            let Some(exe) = process.exe().and_then(|value| value.to_str()) else {
                continue;
            };
            let normalized = normalize_path_for_compare(exe);
            if normalized.is_empty() {
                continue;
            }
            map.insert(pid.as_u32(), normalized);
        }
    }

    map
}

fn filter_entries_by_expected_launch_path(
    app_label: &str,
    entries: Vec<(u32, Option<String>)>,
    expected: Option<String>,
) -> Vec<(u32, Option<String>)> {
    if entries.is_empty() {
        return entries;
    }
    let Some(expected) = expected else {
        return Vec::new();
    };
    let exe_by_pid = collect_running_process_exe_by_pid();
    #[cfg(target_os = "macos")]
    let expected_app_root = normalize_macos_app_root(std::path::Path::new(&expected))
        .map(|root| normalize_path_for_compare(&root))
        .filter(|root| !root.is_empty());
    let mut result = Vec::new();
    let mut missing_exe = 0usize;
    let mut path_mismatch = 0usize;
    #[cfg(target_os = "macos")]
    let mut app_root_match = 0usize;
    for (pid, dir) in entries {
        match exe_by_pid.get(&pid) {
            Some(actual) if actual == &expected => result.push((pid, dir)),
            Some(actual) => {
                #[cfg(target_os = "macos")]
                {
                    if let Some(expected_root) = expected_app_root.as_ref() {
                        let actual_root = normalize_macos_app_root(std::path::Path::new(actual))
                            .map(|root| normalize_path_for_compare(&root))
                            .filter(|root| !root.is_empty());
                        if actual_root.as_ref() == Some(expected_root) {
                            app_root_match += 1;
                            result.push((pid, dir));
                            continue;
                        }
                    }
                }
                path_mismatch += 1;
            }
            None => missing_exe += 1,
        }
    }
    if result.is_empty() {
        #[cfg(target_os = "macos")]
        {
            crate::modules::logger::log_warn(&format!(
                "[{} Resolve] 启动路径硬匹配未命中：expected={}, path_mismatch={}, missing_exe={}, app_root_match={}",
                app_label, expected, path_mismatch, missing_exe, app_root_match
            ));
        }
        #[cfg(not(target_os = "macos"))]
        crate::modules::logger::log_warn(&format!(
            "[{} Resolve] 启动路径硬匹配未命中：expected={}, path_mismatch={}, missing_exe={}",
            app_label, expected, path_mismatch, missing_exe
        ));
    } else {
        #[cfg(target_os = "macos")]
        if app_root_match > 0 {
            crate::modules::logger::log_info(&format!(
                "[{} Resolve] 使用 .app 根路径匹配到进程：expected={}, app_root_match={}",
                app_label, expected, app_root_match
            ));
        }
    }
    result
}

fn resolve_expected_antigravity_launch_path_for_match() -> Option<String> {
    let launch_path = match resolve_antigravity_launch_path() {
        Ok(path) => path,
        Err(err) => {
            crate::modules::logger::log_warn(&format!(
                "[AG Resolve] 启动路径未配置或无效，跳过 PID 匹配: {}",
                err
            ));
            return None;
        }
    };
    let normalized = normalize_path_for_compare(launch_path.to_string_lossy().as_ref());
    if normalized.is_empty() {
        crate::modules::logger::log_warn("[AG Resolve] 启动路径为空，跳过 PID 匹配");
        return None;
    }
    Some(normalized)
}

fn resolve_expected_vscode_launch_path_for_match() -> Option<String> {
    let launch_path = match resolve_vscode_launch_path() {
        Ok(path) => path,
        Err(err) => {
            crate::modules::logger::log_warn(&format!(
                "[VSCode Resolve] 启动路径未配置或无效，跳过 PID 匹配: {}",
                err
            ));
            return None;
        }
    };
    let normalized = normalize_path_for_compare(launch_path.to_string_lossy().as_ref());
    if normalized.is_empty() {
        crate::modules::logger::log_warn("[VSCode Resolve] 启动路径为空，跳过 PID 匹配");
        return None;
    }
    Some(normalized)
}

#[cfg(target_os = "macos")]
fn resolve_expected_codex_launch_path_for_match() -> Option<String> {
    let launch_path = match resolve_codex_launch_path() {
        Ok(path) => path,
        Err(err) => {
            crate::modules::logger::log_warn(&format!(
                "[Codex Resolve] 启动路径未配置或无效，跳过 PID 匹配: {}",
                err
            ));
            return None;
        }
    };
    let normalized = normalize_path_for_compare(launch_path.to_string_lossy().as_ref());
    if normalized.is_empty() {
        crate::modules::logger::log_warn("[Codex Resolve] 启动路径为空，跳过 PID 匹配");
        return None;
    }
    Some(normalized)
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
fn collect_antigravity_process_entries_from_powershell(
    expected_exe_path: &str,
) -> Vec<(u32, Option<String>)> {
    let mut result = Vec::new();
    let script = build_windows_path_filtered_process_probe_script(
        "Antigravity.exe",
        expected_exe_path,
    );
    let output = powershell_output_with_timeout(
        &["-NoProfile", "-Command", &script],
        WINDOWS_PROCESS_PROBE_TIMEOUT,
    );
    let output = match output {
        Ok(value) => value,
        Err(err) => {
            if err.kind() == std::io::ErrorKind::TimedOut {
                crate::modules::logger::log_warn(
                    "[AG Probe] PowerShell 进程探测超时（5s）",
                );
            } else {
                crate::modules::logger::log_warn(&format!(
                    "[AG Probe] PowerShell 进程探测失败: {}",
                    err
                ));
            }
            return result;
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        crate::modules::logger::log_warn(&format!(
            "[AG Probe] PowerShell 进程探测返回非 0 状态: {}, stderr={}",
            output.status,
            stderr.trim()
        ));
        return result;
    }
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
        if !is_antigravity_main_process("antigravity.exe", "", Some(&lower)) {
            continue;
        }
        let dir = extract_user_data_dir_from_command_line(cmdline);
        result.push((pid, dir));
    }
    result
}

#[cfg(target_os = "windows")]
fn resolve_windows_process_exe_for_match(process: &sysinfo::Process) -> (Option<String>, bool) {
    if let Some(exe) = process.exe().and_then(|value| value.to_str()) {
        let normalized = normalize_path_for_compare(exe);
        if !normalized.is_empty() {
            return (Some(normalized), false);
        }
    }
    if let Some(first) = process.cmd().first() {
        let normalized = normalize_path_for_compare(first.to_string_lossy().as_ref());
        if !normalized.is_empty() {
            return (Some(normalized), true);
        }
    }
    (None, false)
}

#[cfg(target_os = "windows")]
fn collect_antigravity_process_entries_from_sysinfo_fallback(
    expected_exe_path: &str,
) -> Vec<(u32, Option<String>)> {
    let expected = normalize_path_for_compare(expected_exe_path);
    if expected.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut candidates = 0usize;
    let mut path_mismatch = 0usize;
    let mut missing_exe = 0usize;
    let mut cmdline_fallback_hit = 0usize;

    let mut system = System::new();
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet),
    );
    let current_pid = std::process::id();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        if pid_u32 == current_pid {
            continue;
        }

        let name = process.name().to_string_lossy().to_lowercase();
        let exe_path = process
            .exe()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_lowercase();
        let args = process.cmd();
        let args_lower = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_lowercase())
            .collect::<Vec<String>>()
            .join(" ");

        if !is_antigravity_main_process(&name, &exe_path, Some(&args_lower)) {
            continue;
        }
        candidates += 1;

        let (actual, used_cmdline_fallback) = resolve_windows_process_exe_for_match(process);
        match actual {
            Some(actual_path) if actual_path == expected => {
                if used_cmdline_fallback {
                    cmdline_fallback_hit += 1;
                }
                let dir = extract_user_data_dir(args);
                result.push((pid_u32, dir));
            }
            Some(_) => path_mismatch += 1,
            None => missing_exe += 1,
        }
    }

    if result.is_empty() {
        crate::modules::logger::log_warn(&format!(
            "[AG Probe] sysinfo fallback no match: expected={}, candidates={}, path_mismatch={}, missing_exe={}, cmdline_fallback_hit={}",
            expected, candidates, path_mismatch, missing_exe, cmdline_fallback_hit
        ));
    } else {
        crate::modules::logger::log_info(&format!(
            "[AG Probe] sysinfo fallback matched: expected={}, matched={}, candidates={}, path_mismatch={}, missing_exe={}, cmdline_fallback_hit={}",
            expected, result.len(), candidates, path_mismatch, missing_exe, cmdline_fallback_hit
        ));
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
    let expected_launch = resolve_expected_antigravity_launch_path_for_match();
    if expected_launch.is_none() {
        return Vec::new();
    }

    #[cfg(target_os = "macos")]
    {
        let entries = collect_antigravity_process_entries_macos();
        if !entries.is_empty() {
            return filter_entries_by_expected_launch_path(
                "AG",
                entries,
                expected_launch.clone(),
            );
        }
        let entries = collect_antigravity_process_entries_from_ps();
        if !entries.is_empty() {
            return filter_entries_by_expected_launch_path(
                "AG",
                entries,
                expected_launch.clone(),
            );
        }
        // macOS 下避免回退到 sysinfo，防止触发 TCC「其他 App 数据」授权弹窗
        return Vec::new();
    }

    #[cfg(target_os = "windows")]
    {
        let expected = expected_launch
            .as_deref()
            .expect("expected launch path must exist");
        let entries = collect_antigravity_process_entries_from_powershell(expected);
        if !entries.is_empty() {
            return entries;
        }
        if strict_process_detect_enabled() {
            crate::modules::logger::log_warn(
                "[AG Probe] strict mode enabled and PowerShell returned empty; skip sysinfo fallback",
            );
            return Vec::new();
        }
        crate::modules::logger::log_warn(
            "[AG Probe] PowerShell returned empty; fallback to sysinfo probe",
        );
        return collect_antigravity_process_entries_from_sysinfo_fallback(expected);
    }

    #[cfg(target_os = "linux")]
    {
        let entries = collect_antigravity_process_entries_from_proc();
        if !entries.is_empty() {
            return filter_entries_by_expected_launch_path(
                "AG",
                entries,
                expected_launch.clone(),
            );
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut result = Vec::new();
        let mut system = System::new();
        system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet));

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

            if !is_antigravity_main_process(&name, &exe_path, Some(&args_str)) {
                continue;
            }

            let dir = extract_user_data_dir(&args);
            result.push((pid_u32, dir));
        }

        return filter_entries_by_expected_launch_path(
            "AG",
            result,
            expected_launch,
        );
    }
}

fn pick_preferred_pid(mut pids: Vec<u32>) -> Option<u32> {
    if pids.is_empty() {
        return None;
    }
    pids.sort();
    pids.dedup();
    pids.first().copied()
}

fn normalize_non_empty_path_for_compare(value: &str) -> Option<String> {
    let normalized = normalize_path_for_compare(value);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn build_user_data_dir_match_target(
    requested_user_data_dir: Option<&str>,
    default_user_data_dir: Option<String>,
    fallback_to_default_when_missing: bool,
) -> Option<(String, bool)> {
    let requested_target =
        requested_user_data_dir.and_then(|value| normalize_non_empty_path_for_compare(value));
    let default_target =
        default_user_data_dir.and_then(|value| normalize_non_empty_path_for_compare(&value));
    let target = requested_target.or_else(|| {
        if fallback_to_default_when_missing {
            default_target.clone()
        } else {
            None
        }
    })?;
    let allow_none_for_target = default_target
        .as_ref()
        .map(|value| value == &target)
        .unwrap_or(false);
    Some((target, allow_none_for_target))
}

fn collect_matching_pids_by_user_data_dir(
    entries: &[(u32, Option<String>)],
    target_dir: &str,
    allow_none_for_target: bool,
) -> Vec<u32> {
    let mut matches = Vec::new();
    for (pid, dir) in entries {
        match dir.as_ref() {
            Some(value) => {
                let normalized = normalize_path_for_compare(value);
                if !normalized.is_empty() && normalized == target_dir {
                    matches.push(*pid);
                }
            }
            None => {
                if allow_none_for_target {
                    matches.push(*pid);
                }
            }
        }
    }
    matches
}

fn resolve_pid_from_entries_by_user_data_dir(
    last_pid: Option<u32>,
    target_dir: &str,
    allow_none_for_target: bool,
    entries: &[(u32, Option<String>)],
) -> Option<u32> {
    if target_dir.is_empty() {
        return None;
    }

    let matches =
        collect_matching_pids_by_user_data_dir(entries, target_dir, allow_none_for_target);

    if let Some(pid) = last_pid {
        if is_pid_running(pid) && matches.contains(&pid) {
            return Some(pid);
        }
        if is_pid_running(pid) {
            crate::modules::logger::log_warn(&format!(
                "[PID Resolve] 忽略不匹配的 last_pid={}，target={}，matched_pids={:?}",
                pid, target_dir, matches
            ));
        }
    }

    pick_preferred_pid(matches)
}

fn get_default_antigravity_user_data_dir() -> Option<String> {
    crate::modules::instance::get_default_user_data_dir()
        .ok()
        .map(|value| normalize_path_for_compare(&value.to_string_lossy()))
        .filter(|value| !value.is_empty())
}

fn resolve_antigravity_target_and_fallback(user_data_dir: Option<&str>) -> Option<(String, bool)> {
    build_user_data_dir_match_target(
        user_data_dir,
        get_default_antigravity_user_data_dir(),
        !strict_process_detect_enabled(),
    )
}

fn resolve_vscode_target_and_fallback(user_data_dir: Option<&str>) -> Option<(String, bool)> {
    build_user_data_dir_match_target(
        user_data_dir,
        get_default_vscode_user_data_dir_for_os(),
        !strict_process_detect_enabled(),
    )
}

pub fn resolve_antigravity_pid_from_entries(
    last_pid: Option<u32>,
    user_data_dir: Option<&str>,
    entries: &[(u32, Option<String>)],
) -> Option<u32> {
    let (target, allow_none_for_target) = resolve_antigravity_target_and_fallback(user_data_dir)?;
    let matches = collect_matching_pids_by_user_data_dir(entries, &target, allow_none_for_target);

    if let Some(pid) = last_pid {
        if is_pid_running(pid) && matches.contains(&pid) {
            return Some(pid);
        }
        if is_pid_running(pid) {
            crate::modules::logger::log_warn(&format!(
                "[AG Resolve] 忽略不匹配的 last_pid={}，target={}，matched_pids={:?}",
                pid, target, matches
            ));
        }
    }

    pick_preferred_pid(matches)
}

pub fn resolve_antigravity_pid(last_pid: Option<u32>, user_data_dir: Option<&str>) -> Option<u32> {
    let entries = collect_antigravity_process_entries();
    resolve_antigravity_pid_from_entries(last_pid, user_data_dir, &entries)
}

#[cfg(target_os = "macos")]
fn focus_window_by_pid(pid: u32) -> Result<(), String> {
    let script = format!(
        "tell application \"System Events\" to set frontmost of (first process whose unix id is {}) to true",
        pid
    );
    crate::modules::logger::log_info(&format!("[Focus] macOS osascript start pid={}", pid));
    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("调用 osascript 失败: {}", e))?;
    if output.status.success() {
        crate::modules::logger::log_info(&format!("[Focus] macOS osascript success pid={}", pid));
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
        r#"$targetPid={pid};$h=[IntPtr]::Zero;for($i=0;$i -lt 20;$i++){{$p=Get-Process -Id $targetPid -ErrorAction Stop;$h=$p.MainWindowHandle;if ($h -ne 0) {{ break }};Start-Sleep -Milliseconds 150}};if ($h -eq 0) {{ throw 'MAIN_WINDOW_HANDLE_EMPTY' }};Add-Type @' 
using System; 
using System.Runtime.InteropServices; 
public class Win32 {{ 
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd); 
  [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow); 
}} 
'@;[Win32]::ShowWindowAsync($h, 9) | Out-Null;[Win32]::SetForegroundWindow($h) | Out-Null;"#
    );
    crate::modules::logger::log_info(&format!("[Focus] Windows PowerShell start pid={}", pid));
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
                        let focus = Command::new("wmctrl").args(["-ia", win_id]).output();
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
        crate::modules::logger::log_info(&format!("[Focus] Linux xdotool success pid={}", pid));
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

    if let Some(pid) = last_pid {
        if is_pid_running(pid) && matches.contains(&pid) {
            return Some(pid);
        }
        if is_pid_running(pid) {
            crate::modules::logger::log_warn(&format!(
                "[Codex Resolve] 忽略不匹配的 last_pid={}，target={:?}，matched_pids={:?}",
                pid, target, matches
            ));
        }
    }

    pick_preferred_pid(matches)
}

#[cfg(not(target_os = "macos"))]
pub fn resolve_codex_pid_from_entries(
    last_pid: Option<u32>,
    _codex_home: Option<&str>,
    _entries: &[(u32, Option<String>)],
) -> Option<u32> {
    last_pid.filter(|pid| is_pid_running(*pid))
}

#[cfg(target_os = "macos")]
pub fn resolve_codex_pid(last_pid: Option<u32>, codex_home: Option<&str>) -> Option<u32> {
    let entries = collect_codex_process_entries();
    resolve_codex_pid_from_entries(last_pid, codex_home, &entries)
}

#[cfg(not(target_os = "macos"))]
pub fn resolve_codex_pid(last_pid: Option<u32>, _codex_home: Option<&str>) -> Option<u32> {
    last_pid.filter(|pid| is_pid_running(*pid))
}

pub fn focus_codex_instance(
    last_pid: Option<u32>,
    codex_home: Option<&str>,
) -> Result<u32, String> {
    let resolve_start = Instant::now();
    let pid = resolve_codex_pid(last_pid, codex_home)
        .ok_or_else(|| "实例未运行，无法定位窗口".to_string())?;
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

#[cfg(target_os = "windows")]
fn collect_vscode_process_entries_from_powershell(
    expected_exe_path: &str,
) -> Vec<(u32, Option<String>)> {
    let mut entries: Vec<(u32, Option<String>)> = Vec::new();
    let script = build_windows_path_filtered_process_probe_script("Code.exe", expected_exe_path);
    let output = powershell_output(&["-Command", &script]);
    let output = match output {
        Ok(value) => value,
        Err(_) => return entries,
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
        if is_helper_command_line(&lower) || lower.contains("crashpad_handler") {
            continue;
        }
        let dir = extract_user_data_dir_from_command_line(cmdline).and_then(|value| {
            let normalized = normalize_path_for_compare(&value);
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        });
        entries.push((pid, dir));
    }
    entries.sort_by_key(|(pid, _)| *pid);
    entries.dedup_by(|a, b| a.0 == b.0);
    entries
}

#[cfg(target_os = "windows")]
fn collect_vscode_process_entries_from_sysinfo_fallback(
    expected_exe_path: &str,
) -> Vec<(u32, Option<String>)> {
    let expected = normalize_path_for_compare(expected_exe_path);
    if expected.is_empty() {
        return Vec::new();
    }

    let mut entries: Vec<(u32, Option<String>)> = Vec::new();
    let mut candidates = 0usize;
    let mut path_mismatch = 0usize;
    let mut missing_exe = 0usize;
    let mut cmdline_fallback_hit = 0usize;

    let mut system = System::new();
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet),
    );
    let current_pid = std::process::id();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        if pid_u32 == current_pid {
            continue;
        }

        let name = process.name().to_string_lossy().to_lowercase();
        let exe_path = process
            .exe()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_lowercase();
        let args_line = process
            .cmd()
            .iter()
            .map(|arg| arg.to_string_lossy().to_lowercase())
            .collect::<Vec<String>>()
            .join(" ");
        let is_vscode = name == "code.exe" || exe_path.ends_with("\\code.exe");
        if !is_vscode || is_helper_command_line(&args_line) || args_line.contains("crashpad_handler")
        {
            continue;
        }
        candidates += 1;

        let (actual, used_cmdline_fallback) = resolve_windows_process_exe_for_match(process);
        match actual {
            Some(actual_path) if actual_path == expected => {
                if used_cmdline_fallback {
                    cmdline_fallback_hit += 1;
                }
                let dir = extract_user_data_dir(process.cmd()).and_then(|value| {
                    let normalized = normalize_path_for_compare(&value);
                    if normalized.is_empty() {
                        None
                    } else {
                        Some(normalized)
                    }
                });
                entries.push((pid_u32, dir));
            }
            Some(_) => path_mismatch += 1,
            None => missing_exe += 1,
        }
    }

    entries.sort_by_key(|(pid, _)| *pid);
    entries.dedup_by(|a, b| a.0 == b.0);

    if entries.is_empty() {
        crate::modules::logger::log_warn(&format!(
            "[VSCode Probe] sysinfo fallback no match: expected={}, candidates={}, path_mismatch={}, missing_exe={}, cmdline_fallback_hit={}",
            expected, candidates, path_mismatch, missing_exe, cmdline_fallback_hit
        ));
    } else {
        crate::modules::logger::log_info(&format!(
            "[VSCode Probe] sysinfo fallback matched: expected={}, matched={}, candidates={}, path_mismatch={}, missing_exe={}, cmdline_fallback_hit={}",
            expected, entries.len(), candidates, path_mismatch, missing_exe, cmdline_fallback_hit
        ));
    }

    entries
}

pub fn collect_vscode_process_entries() -> Vec<(u32, Option<String>)> {
    let expected_launch = resolve_expected_vscode_launch_path_for_match();
    if expected_launch.is_none() {
        return Vec::new();
    }

    #[cfg(target_os = "windows")]
    {
        let expected = expected_launch
            .as_deref()
            .expect("expected launch path must exist");
        let entries = collect_vscode_process_entries_from_powershell(expected);
        if !entries.is_empty() {
            return entries;
        }
        crate::modules::logger::log_warn(
            "[VSCode Probe] PowerShell returned empty; fallback to sysinfo probe",
        );
        return collect_vscode_process_entries_from_sysinfo_fallback(expected);
    }

    let mut entries = Vec::new();

    // On macOS, only use ps to avoid sysinfo TCC dialogs
    #[cfg(not(target_os = "macos"))]
    {
        let mut system = System::new();
        system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet));

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
    filter_entries_by_expected_launch_path(
        "VSCode",
        result,
        expected_launch,
    )
}

pub fn resolve_vscode_pid_from_entries(
    last_pid: Option<u32>,
    user_data_dir: Option<&str>,
    entries: &[(u32, Option<String>)],
) -> Option<u32> {
    let (target, allow_none_for_target) = resolve_vscode_target_and_fallback(user_data_dir)?;
    resolve_pid_from_entries_by_user_data_dir(last_pid, &target, allow_none_for_target, entries)
}

pub fn resolve_vscode_pid(last_pid: Option<u32>, user_data_dir: Option<&str>) -> Option<u32> {
    let entries = collect_vscode_process_entries();
    resolve_vscode_pid_from_entries(last_pid, user_data_dir, &entries)
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
        return Some(
            Path::new(&appdata)
                .join("Code")
                .to_string_lossy()
                .to_string(),
        );
    }

    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir()?;
        return Some(
            home.join(".config")
                .join("Code")
                .to_string_lossy()
                .to_string(),
        );
    }

    #[allow(unreachable_code)]
    None
}

pub fn focus_vscode_instance(
    last_pid: Option<u32>,
    user_data_dir: Option<&str>,
) -> Result<u32, String> {
    let resolve_start = Instant::now();
    let pid = resolve_vscode_pid(last_pid, user_data_dir)
        .ok_or_else(|| "实例未运行，无法定位窗口".to_string())?;
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

    // On macOS, skip sysinfo to avoid TCC dialogs
    #[cfg(not(target_os = "macos"))]
    {
        let mut system = System::new();
        system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet));

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

            if !is_antigravity_main_process(&name, &exe_path, Some(&args_str)) {
                continue;
            }

            if let Some(dir) = extract_user_data_dir(&args) {
                let normalized = normalize_path_for_compare(&dir);
                if normalized == target {
                    result.push(pid_u32);
                }
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
                let lower = cmdline.to_lowercase();
                if !is_antigravity_main_process("antigravity.exe", "", Some(&lower)) {
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
    let mut result = Vec::new();

    // On macOS, skip sysinfo to avoid TCC dialogs
    #[cfg(not(target_os = "macos"))]
    {
        let mut system = System::new();
        system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet));

        let current_pid = std::process::id();

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
    let mut pids = Vec::new();

    #[cfg(target_os = "macos")]
    {
        // Use ps to avoid sysinfo TCC dialogs on macOS
        if let Ok(output) = Command::new("ps").args(["-axww", "-o", "pid=,command="]).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if line.is_empty() { continue; }
                let mut parts = line.splitn(2, |ch: char| ch.is_whitespace());
                let pid_str = parts.next().unwrap_or("").trim();
                let cmdline = parts.next().unwrap_or("").trim();
                let pid = match pid_str.parse::<u32>() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let lower = cmdline.to_lowercase();
                if !lower.contains("antigravity.app/contents/") { continue; }
                if lower.contains("antigravity tools.app/contents/") { continue; }
                // Include both main and helper processes (this function collects all)
                pids.push(pid);
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut system = System::new();
        system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet));

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

    crate::modules::logger::log_info(&format!("准备关闭 {} 个 Antigravity 主进程...", pids.len()));
    let _ = close_pids(&pids, timeout_secs);

    if is_antigravity_running() {
        return Err("无法关闭 Antigravity 进程，请手动关闭后重试".to_string());
    }

    crate::modules::logger::log_info("Antigravity 已成功关闭");
    Ok(())
}

fn collect_remaining_pids(entries: &[(u32, Option<String>)]) -> Vec<u32> {
    let mut pids: Vec<u32> = entries.iter().map(|(pid, _)| *pid).collect();
    pids.sort();
    pids.dedup();
    pids
}

fn resolve_entry_user_data_dir_for_matching(
    dir: Option<&String>,
    default_dir: Option<&str>,
) -> Option<String> {
    dir.and_then(|value| normalize_non_empty_path_for_compare(value))
        .or_else(|| default_dir.and_then(normalize_non_empty_path_for_compare))
}

fn entry_matches_target_dirs(
    dir: Option<&String>,
    target_dirs: &HashSet<String>,
    default_dir: Option<&str>,
) -> bool {
    resolve_entry_user_data_dir_for_matching(dir, default_dir)
        .map(|value| target_dirs.contains(&value))
        .unwrap_or(false)
}

fn select_main_pids_by_target_dirs(
    entries: &[(u32, Option<String>)],
    target_dirs: &HashSet<String>,
    default_dir: Option<&str>,
) -> Vec<u32> {
    entries
        .iter()
        .filter_map(|(pid, dir)| {
            entry_matches_target_dirs(dir.as_ref(), target_dirs, default_dir).then_some(*pid)
        })
        .collect()
}

fn filter_entries_by_target_dirs(
    entries: Vec<(u32, Option<String>)>,
    target_dirs: &HashSet<String>,
    default_dir: Option<&str>,
) -> Vec<(u32, Option<String>)> {
    entries
        .into_iter()
        .filter(|(_, dir)| entry_matches_target_dirs(dir.as_ref(), target_dirs, default_dir))
        .collect()
}

fn close_managed_instances_common<CollectEntries, SelectMainPids, CollectRemainingEntries>(
    log_prefix: &str,
    start_message: &str,
    empty_targets_message: &str,
    not_running_message: &str,
    process_display_name: &str,
    failure_message: &str,
    user_data_dirs: &[String],
    timeout_secs: u64,
    collect_entries: CollectEntries,
    select_main_pids: SelectMainPids,
    collect_remaining_entries: CollectRemainingEntries,
    graceful_close: Option<fn(u32)>,
    graceful_wait_secs: Option<u64>,
    detail_logger: Option<fn(&[u32])>,
) -> Result<(), String>
where
    CollectEntries: Fn() -> Vec<(u32, Option<String>)>,
    SelectMainPids: Fn(&[(u32, Option<String>)], &HashSet<String>) -> Vec<u32>,
    CollectRemainingEntries: Fn(&HashSet<String>) -> Vec<(u32, Option<String>)>,
{
    crate::modules::logger::log_info(start_message);

    let target_dirs: HashSet<String> = user_data_dirs
        .iter()
        .map(|value| normalize_path_for_compare(value))
        .filter(|value| !value.is_empty())
        .collect();
    if target_dirs.is_empty() {
        crate::modules::logger::log_info(empty_targets_message);
        return Ok(());
    }
    crate::modules::logger::log_info(&format!(
        "[{}] target_dirs={:?}, timeout_secs={}",
        log_prefix, target_dirs, timeout_secs
    ));

    let entries = collect_entries();
    crate::modules::logger::log_info(&format!("[{}] collected_entries={:?}", log_prefix, entries));

    let mut pids = select_main_pids(&entries, &target_dirs);
    pids.sort();
    pids.dedup();
    if pids.is_empty() {
        crate::modules::logger::log_info(not_running_message);
        return Ok(());
    }
    crate::modules::logger::log_info(&format!("[{}] matched_main_pids={:?}", log_prefix, pids));

    crate::modules::logger::log_info(&format!(
        "准备关闭 {} 个{}主进程...",
        pids.len(),
        process_display_name
    ));

    if let Some(graceful_close_fn) = graceful_close {
        for pid in &pids {
            graceful_close_fn(*pid);
        }
        if let Some(wait_secs) = graceful_wait_secs {
            if wait_pids_exit(&pids, wait_secs) {
                crate::modules::logger::log_info(&format!(
                    "[{}] graceful close finished, targets={:?}",
                    log_prefix, pids
                ));
                return Ok(());
            }
        }
    }

    if let Err(err) = close_pids(&pids, timeout_secs) {
        crate::modules::logger::log_warn(&format!(
            "[{}] close_pids returned error: {}",
            log_prefix, err
        ));
    }

    let mut remaining_entries = collect_remaining_entries(&target_dirs);
    if !remaining_entries.is_empty() {
        let remaining_pids = collect_remaining_pids(&remaining_entries);
        crate::modules::logger::log_warn(&format!(
            "[{}] first remaining pids after close={:?}",
            log_prefix, remaining_pids
        ));
        if let Some(detail_logger_fn) = detail_logger {
            detail_logger_fn(&remaining_pids);
        }
        if !remaining_pids.is_empty() {
            crate::modules::logger::log_warn(&format!(
                "[{}] retry force close for remaining pids={:?}",
                log_prefix, remaining_pids
            ));
            if let Err(err) = close_pids(&remaining_pids, 6) {
                crate::modules::logger::log_warn(&format!(
                    "[{}] retry close_pids returned error: {}",
                    log_prefix, err
                ));
            }
            remaining_entries = collect_remaining_entries(&target_dirs);
        }
    }

    if !remaining_entries.is_empty() {
        let remaining_pids = collect_remaining_pids(&remaining_entries);
        if let Some(detail_logger_fn) = detail_logger {
            detail_logger_fn(&remaining_pids);
        }
        crate::modules::logger::log_error(&format!(
            "[{}] still_running_entries={:?}",
            log_prefix, remaining_entries
        ));
        return Err(format!(
            "{} (remaining_pids={:?})",
            failure_message, remaining_pids
        ));
    }

    Ok(())
}

/// 关闭受管 Antigravity 实例（按 user-data-dir 匹配，包含默认实例目录）
pub fn close_antigravity_instances(
    user_data_dirs: &[String],
    timeout_secs: u64,
) -> Result<(), String> {
    let default_dir = crate::modules::instance::get_default_user_data_dir()
        .ok()
        .map(|value| normalize_path_for_compare(&value.to_string_lossy()))
        .filter(|value| !value.is_empty());
    crate::modules::logger::log_info(&format!("[AG Close] default_dir={:?}", default_dir));
    close_managed_instances_common(
        "AG Close",
        "正在关闭受管 Antigravity 实例...",
        "未提供可关闭的 Antigravity 实例目录",
        "受管 Antigravity 实例未在运行，无需关闭",
        "受管 Antigravity ",
        "无法关闭受管 Antigravity 实例进程，请手动关闭后重试",
        user_data_dirs,
        timeout_secs,
        collect_antigravity_process_entries,
        |entries, target_dirs| {
            select_main_pids_by_target_dirs(entries, target_dirs, default_dir.as_deref())
        },
        |target_dirs| {
            filter_entries_by_target_dirs(
                collect_antigravity_process_entries(),
                target_dirs,
                default_dir.as_deref(),
            )
        },
        None,
        None,
        #[cfg(target_os = "windows")]
        Some(log_antigravity_process_details_for_pids as fn(&[u32])),
        #[cfg(not(target_os = "windows"))]
        None,
    )
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

        crate::modules::logger::log_info(&format!("[AG Close] taskkill start pid={}", pid));
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
        let _ = Command::new("kill")
            .args(["-15", &pid.to_string()])
            .output();
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

#[cfg(target_os = "windows")]
fn log_vscode_process_details_for_pids(pids: &[u32]) {
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
        "$ids=@({}); Get-CimInstance Win32_Process -Filter \"Name='Code.exe'\" | Where-Object {{$ids -contains $_.ProcessId}} | ForEach-Object {{ \"$($_.ProcessId)|$($_.ParentProcessId)|$($_.CommandLine)\" }}",
        pid_list
    );
    match powershell_output(&["-Command", &script]) {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().is_empty() {
                crate::modules::logger::log_warn(&format!(
                    "[VSCode Close] remaining pid details not found for {:?}",
                    unique
                ));
            } else {
                for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
                    crate::modules::logger::log_warn(&format!(
                        "[VSCode Close] remaining_pid_detail {}",
                        line.trim()
                    ));
                }
            }
        }
        Err(err) => {
            crate::modules::logger::log_warn(&format!(
                "[VSCode Close] read remaining pid details failed: {}",
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
        crate::modules::logger::log_info(&format!("[ClosePids] all exited, targets={:?}", targets));
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
pub fn start_antigravity_with_args(
    user_data_dir: &str,
    extra_args: &[String],
) -> Result<u32, String> {
    crate::modules::logger::log_info("正在启动 Antigravity...");

    #[cfg(target_os = "macos")]
    let launch_path = resolve_antigravity_launch_path().ok();
    #[cfg(not(target_os = "macos"))]
    let launch_path = resolve_antigravity_launch_path()?;

    #[cfg(target_os = "macos")]
    {
        let app_root = resolve_macos_app_root_from_config("antigravity").or_else(|| {
            launch_path
                .as_ref()
                .and_then(|path| normalize_macos_app_root(path))
        });
        let app_root = app_root.ok_or_else(|| app_path_missing_error("antigravity"))?;

        let user_data_dir_trimmed = user_data_dir.trim();
        let force_new_instance = !user_data_dir_trimmed.is_empty();
        let mut args: Vec<String> = Vec::new();
        if force_new_instance {
            args.push("--user-data-dir".to_string());
            args.push(user_data_dir_trimmed.to_string());
        }
        if !force_new_instance {
            args.push("--reuse-window".to_string());
        }
        for arg in extra_args {
            if !arg.trim().is_empty() {
                args.push(arg.to_string());
            }
        }
        let pid = spawn_open_app_with_options(&app_root, &args, force_new_instance)
            .map_err(|e| format!("启动 Antigravity 失败: {}", e))?;
        crate::modules::logger::log_info("Antigravity 启动命令已发送");
        if force_new_instance {
            let probe_started = Instant::now();
            let timeout = Duration::from_secs(6);
            while probe_started.elapsed() < timeout {
                if let Some(resolved_pid) =
                    resolve_antigravity_pid(None, Some(user_data_dir_trimmed))
                {
                    return Ok(resolved_pid);
                }
                thread::sleep(Duration::from_millis(200));
            }
            crate::modules::logger::log_warn(&format!(
                "[AG Start] 启动后 6s 内未匹配到实例 PID，回退 open pid={}",
                pid
            ));
        }
        return Ok(pid);
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
        let child = spawn_command_with_trace(&mut cmd)
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
        let child =
            spawn_detached_unix(&mut cmd).map_err(|e| format!("启动 Antigravity 失败: {}", e))?;
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
    let expected_launch = resolve_expected_codex_launch_path_for_match();
    if expected_launch.is_none() {
        return Vec::new();
    }

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
            if !cmdline
                .to_lowercase()
                .contains("codex.app/contents/macos/codex")
            {
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
    filter_entries_by_expected_launch_path(
        "Codex",
        result,
        expected_launch,
    )
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
            sanitize_macos_gui_launch_env(&mut cmd);
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

/// 启动 Codex 默认实例（不注入 CODEX_HOME/额外参数，支持 macOS / Windows）
pub fn start_codex_default() -> Result<u32, String> {
    #[cfg(target_os = "macos")]
    {
        let app_root = resolve_macos_app_root_from_config("codex");
        if let Ok(launch_path) = resolve_codex_launch_path() {
            let mut cmd = Command::new(&launch_path);
            sanitize_macos_gui_launch_env(&mut cmd);
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
            let pid =
                spawn_open_app(&app_root, &[]).map_err(|e| format!("启动 Codex 失败: {}", e))?;
            crate::modules::logger::log_info("Codex 启动命令已发送");
            return Ok(pid);
        }
        return Err(app_path_missing_error("codex"));
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        let launch_path = resolve_codex_launch_path()?;
        let mut cmd = Command::new(&launch_path);
        if should_detach_child() {
            cmd.creation_flags(0x08000000 | CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
        } else {
            cmd.creation_flags(0x08000000);
        }

        let child =
            spawn_command_with_trace(&mut cmd).map_err(|e| format!("启动 Codex 失败: {}", e))?;
        crate::modules::logger::log_info("Codex 启动命令已发送");
        return Ok(child.id());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Err("Codex 启动仅支持 macOS 和 Windows".to_string())
}

/// 关闭 Codex 进程（仅 macOS）
#[allow(dead_code)]
pub fn close_codex(timeout_secs: u64) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        crate::modules::logger::log_info("正在关闭 Codex...");
        let pids: Vec<u32> = collect_codex_process_entries()
            .into_iter()
            .map(|(pid, _)| pid)
            .collect();
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
    #[cfg(target_os = "macos")]
    {
        // Use ps to avoid sysinfo TCC dialogs on macOS
        let app_lower = OPENCODE_APP_NAME.to_lowercase();
        let bundle_pattern = format!("{}.app/contents/", app_lower);
        if let Ok(output) = Command::new("ps").args(["-axww", "-o", "command="]).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let lower = line.trim().to_lowercase();
                if lower.contains(&bundle_pattern)
                    && !lower.contains("--type=")
                    && !lower.contains("crashpad_handler")
                {
                    return true;
                }
            }
        }
        return false;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut system = System::new();
        system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet));

        let current_pid = std::process::id();
        #[cfg(target_os = "windows")]
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
}

fn get_opencode_pids() -> Vec<u32> {
    let mut pids = Vec::new();

    #[cfg(target_os = "macos")]
    {
        // Use ps to avoid sysinfo TCC dialogs on macOS
        let app_lower = OPENCODE_APP_NAME.to_lowercase();
        let bundle_pattern = format!("{}.app/contents/", app_lower);
        if let Ok(output) = Command::new("ps").args(["-axww", "-o", "pid=,command="]).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if line.is_empty() { continue; }
                let mut parts = line.splitn(2, |ch: char| ch.is_whitespace());
                let pid_str = parts.next().unwrap_or("").trim();
                let cmdline = parts.next().unwrap_or("").trim();
                let pid = match pid_str.parse::<u32>() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let lower = cmdline.to_lowercase();
                if lower.contains(&bundle_pattern)
                    && !lower.contains("--type=")
                    && !lower.contains("crashpad_handler")
                {
                    pids.push(pid);
                }
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut system = System::new();
        system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet));

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

    crate::modules::logger::log_info(&format!("准备关闭 {} 个 OpenCode 进程...", pids.len()));
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
        let target =
            normalize_custom_path(custom_path).unwrap_or_else(|| OPENCODE_APP_NAME.to_string());

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
            let mut cmd = Command::new(&candidate);
            cmd.creation_flags(0x08000000);
            if spawn_command_with_trace(&mut cmd).is_ok() {
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
            let mut cmd = Command::new(&candidate);
            if spawn_command_with_trace(&mut cmd).is_ok() {
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
        use std::os::windows::process::CommandExt;
        let output = Command::new("netstat")
            .creation_flags(CREATE_NO_WINDOW)
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

    // On macOS, skip sysinfo to avoid TCC dialogs
    #[cfg(not(target_os = "macos"))]
    {
        let mut system = System::new();
        system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet));

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

            #[cfg(target_os = "windows")]
            let is_vscode = name == "code.exe" || exe_path.ends_with("\\code.exe");
            #[cfg(target_os = "linux")]
            let is_vscode = name == "code" || exe_path.ends_with("/code");

            if is_vscode && !is_helper {
                result.push(pid_u32);
            }
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
        sanitize_macos_gui_launch_env(&mut cmd);
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

        let child =
            spawn_detached_unix(&mut cmd).map_err(|e| format!("启动 VS Code 失败: {}", e))?;
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

        let child = spawn_command_with_trace(&mut cmd)
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

        let child =
            spawn_detached_unix(&mut cmd).map_err(|e| format!("启动 VS Code 失败: {}", e))?;
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
        sanitize_macos_gui_launch_env(&mut cmd);
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
        let child =
            spawn_detached_unix(&mut cmd).map_err(|e| format!("启动 VS Code 失败: {}", e))?;
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
        let child = spawn_command_with_trace(&mut cmd)
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
        let child =
            spawn_detached_unix(&mut cmd).map_err(|e| format!("启动 VS Code 失败: {}", e))?;
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
    let default_dir = get_default_vscode_user_data_dir_for_os()
        .map(|value| normalize_path_for_compare(&value))
        .filter(|value| !value.is_empty());
    crate::modules::logger::log_info(&format!("[VSCode Close] default_dir={:?}", default_dir));
    close_managed_instances_common(
        "VSCode Close",
        "正在关闭 VS Code...",
        "未提供可关闭的实例目录",
        "受管 VS Code 实例未在运行，无需关闭",
        "VS Code ",
        "无法关闭受管 VS Code 实例进程，请手动关闭后重试",
        user_data_dirs,
        timeout_secs,
        collect_vscode_process_entries,
        |entries, target_dirs| {
            select_main_pids_by_target_dirs(entries, target_dirs, default_dir.as_deref())
        },
        |target_dirs| {
            filter_entries_by_target_dirs(
                collect_vscode_process_entries(),
                target_dirs,
                default_dir.as_deref(),
            )
        },
        Some(request_vscode_graceful_close as fn(u32)),
        Some(2),
        #[cfg(target_os = "windows")]
        Some(log_vscode_process_details_for_pids as fn(&[u32])),
        #[cfg(not(target_os = "windows"))]
        None,
    )
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
