use std::path::PathBuf;

use crate::settings::{AppSettings, TargetEnv};

fn to_unc_path(distro: &str, linux_path: &str) -> PathBuf {
    // 转换类似 "/home/user/.claude" -> "\\\\wsl$\\<distro>\\home\\user\\.claude"
    let trimmed = linux_path.trim_start_matches('/');
    let win_seg = trimmed.replace('/', "\\");
    let unc = format!("\\\\wsl$\\{}\\{}", distro, win_seg);
    PathBuf::from(unc)
}

pub fn list_wsl_distros_impl() -> Result<Vec<String>, String> {
    #[cfg(windows)]
    {
        let output = std::process::Command::new("wsl.exe")
            .arg("-l")
            .arg("-q")
            .output()
            .map_err(|e| format!("执行 wsl.exe 失败: {}", e))?;
        if !output.status.success() {
            return Err(format!("wsl.exe 返回非零状态: {}", output.status));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut distros = Vec::new();
        for line in stdout.lines() {
            let name = line.trim();
            if !name.is_empty() {
                distros.push(name.to_string());
            }
        }
        Ok(distros)
    }
    #[cfg(not(windows))]
    {
        Ok(vec![])
    }
}

pub fn resolve_wsl_home_impl(distro: &str) -> Result<String, String> {
    #[cfg(windows)]
    {
        let output = std::process::Command::new("wsl.exe")
            .arg("-d")
            .arg(distro)
            .arg("sh")
            .arg("-lc")
            .arg("printf %s \"$HOME\"")
            .output()
            .map_err(|e| format!("执行 wsl.exe 失败: {}", e))?;
        if !output.status.success() {
            return Err(format!("wsl.exe 返回非零状态: {}", output.status));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.trim().to_string())
    }
    #[cfg(not(windows))]
    {
        Err("非 Windows 平台不支持 WSL".to_string())
    }
}

pub fn env_home_path(settings: &AppSettings) -> Result<PathBuf, String> {
    match settings.target_env {
        TargetEnv::Windows => {
            let home = dirs::home_dir().ok_or_else(|| "无法获取用户主目录".to_string())?;
            Ok(home)
        }
        TargetEnv::Wsl => {
            let distro = settings
                .wsl_distro
                .as_ref()
                .ok_or_else(|| "未配置 WSL 发行版".to_string())?;
            let home_linux = resolve_wsl_home_impl(distro)?;
            Ok(to_unc_path(distro, &home_linux))
        }
    }
}

pub fn env_claude_dir(settings: &AppSettings) -> Result<PathBuf, String> {
    Ok(env_home_path(settings)?.join(".claude"))
}

pub fn env_claude_settings_path(settings: &AppSettings) -> Result<PathBuf, String> {
    let dir = env_claude_dir(settings)?;
    let settings = dir.join("settings.json");
    if settings.exists() {
        return Ok(settings);
    }
    let legacy = dir.join("claude.json");
    if legacy.exists() {
        return Ok(legacy);
    }
    Ok(dir.join("settings.json"))
}

pub fn env_codex_dir(settings: &AppSettings) -> Result<PathBuf, String> {
    Ok(env_home_path(settings)?.join(".codex"))
}

pub fn env_codex_auth_path(settings: &AppSettings) -> Result<PathBuf, String> {
    Ok(env_codex_dir(settings)?.join("auth.json"))
}

pub fn env_codex_config_path(settings: &AppSettings) -> Result<PathBuf, String> {
    Ok(env_codex_dir(settings)?.join("config.toml"))
}


