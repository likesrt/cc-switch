#![allow(non_snake_case)]

use std::collections::HashMap;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::app_config::AppType;
use crate::codex_config;
use crate::config::ConfigStatus;
use crate::settings::{load_settings, save_settings as persist_settings};
use crate::wsl_env;
use crate::provider::Provider;
use crate::store::AppState;

/// 获取所有供应商
#[tauri::command]
pub async fn get_providers(
    state: State<'_, AppState>,
    app_type: Option<AppType>,
    app: Option<String>,
    appType: Option<String>,
) -> Result<HashMap<String, Provider>, String> {
    let app_type = app_type
        .or_else(|| app.as_deref().map(|s| s.into()))
        .or_else(|| appType.as_deref().map(|s| s.into()))
        .unwrap_or(AppType::Claude);

    let config = state
        .config
        .lock()
        .map_err(|e| format!("获取锁失败: {}", e))?;

    let manager = config
        .get_manager(&app_type)
        .ok_or_else(|| format!("应用类型不存在: {:?}", app_type))?;

    Ok(manager.get_all_providers().clone())
}

/// 获取当前供应商ID
#[tauri::command]
pub async fn get_current_provider(
    state: State<'_, AppState>,
    app_type: Option<AppType>,
    app: Option<String>,
    appType: Option<String>,
) -> Result<String, String> {
    let app_type = app_type
        .or_else(|| app.as_deref().map(|s| s.into()))
        .or_else(|| appType.as_deref().map(|s| s.into()))
        .unwrap_or(AppType::Claude);

    let config = state
        .config
        .lock()
        .map_err(|e| format!("获取锁失败: {}", e))?;

    let manager = config
        .get_manager(&app_type)
        .ok_or_else(|| format!("应用类型不存在: {:?}", app_type))?;

    Ok(manager.current.clone())
}

/// 添加供应商
#[tauri::command]
pub async fn add_provider(
    state: State<'_, AppState>,
    app_type: Option<AppType>,
    app: Option<String>,
    appType: Option<String>,
    provider: Provider,
) -> Result<bool, String> {
    let app_type = app_type
        .or_else(|| app.as_deref().map(|s| s.into()))
        .or_else(|| appType.as_deref().map(|s| s.into()))
        .unwrap_or(AppType::Claude);

    // 读取当前是否是激活供应商（短锁）
    let is_current = {
        let config = state
            .config
            .lock()
            .map_err(|e| format!("获取锁失败: {}", e))?;
        let manager = config
            .get_manager(&app_type)
            .ok_or_else(|| format!("应用类型不存在: {:?}", app_type))?;
        manager.current == provider.id
    };

    // 若目标为当前供应商，则先写 live，成功后再落盘配置
    if is_current {
        // 依据设置决定写入 Windows 或 WSL 路径
        let settings = load_settings();
        match app_type {
            AppType::Claude => {
                let settings_path = wsl_env::env_claude_settings_path(&settings)?;
                crate::config::write_json_file(&settings_path, &provider.settings_config)?;
            }
            AppType::Codex => {
                let auth = provider
                    .settings_config
                    .get("auth")
                    .ok_or_else(|| "目标供应商缺少 auth 配置".to_string())?;
                let cfg_text = provider
                    .settings_config
                    .get("config")
                    .and_then(|v| v.as_str());
                let auth_path = wsl_env::env_codex_auth_path(&settings)?;
                let cfg_path = wsl_env::env_codex_config_path(&settings)?;
                crate::codex_config::write_codex_live_atomic_at(auth, cfg_text, &auth_path, &cfg_path)?;
            }
        }
    }

    // 更新内存并保存配置
    {
        let mut config = state
            .config
            .lock()
            .map_err(|e| format!("获取锁失败: {}", e))?;
        let manager = config
            .get_manager_mut(&app_type)
            .ok_or_else(|| format!("应用类型不存在: {:?}", app_type))?;
        manager
            .providers
            .insert(provider.id.clone(), provider.clone());
    }
    state.save()?;

    Ok(true)
}

/// 更新供应商
#[tauri::command]
pub async fn update_provider(
    state: State<'_, AppState>,
    app_type: Option<AppType>,
    app: Option<String>,
    appType: Option<String>,
    provider: Provider,
) -> Result<bool, String> {
    let app_type = app_type
        .or_else(|| app.as_deref().map(|s| s.into()))
        .or_else(|| appType.as_deref().map(|s| s.into()))
        .unwrap_or(AppType::Claude);

    // 读取校验 & 是否当前（短锁）
    let (exists, is_current) = {
        let config = state
            .config
            .lock()
            .map_err(|e| format!("获取锁失败: {}", e))?;
        let manager = config
            .get_manager(&app_type)
            .ok_or_else(|| format!("应用类型不存在: {:?}", app_type))?;
        (
            manager.providers.contains_key(&provider.id),
            manager.current == provider.id,
        )
    };
    if !exists {
        return Err(format!("供应商不存在: {}", provider.id));
    }

    // 若更新的是当前供应商，先写 live 成功再保存
    if is_current {
        match app_type {
            AppType::Claude => {
                let settings_path = crate::config::get_claude_settings_path();
                crate::config::write_json_file(&settings_path, &provider.settings_config)?;
            }
            AppType::Codex => {
                let auth = provider
                    .settings_config
                    .get("auth")
                    .ok_or_else(|| "目标供应商缺少 auth 配置".to_string())?;
                let cfg_text = provider
                    .settings_config
                    .get("config")
                    .and_then(|v| v.as_str());
                crate::codex_config::write_codex_live_atomic(auth, cfg_text)?;
            }
        }
    }

    // 更新内存并保存
    {
        let mut config = state
            .config
            .lock()
            .map_err(|e| format!("获取锁失败: {}", e))?;
        let manager = config
            .get_manager_mut(&app_type)
            .ok_or_else(|| format!("应用类型不存在: {:?}", app_type))?;
        manager
            .providers
            .insert(provider.id.clone(), provider.clone());
    }
    state.save()?;

    Ok(true)
}

/// 删除供应商
#[tauri::command]
pub async fn delete_provider(
    state: State<'_, AppState>,
    app_type: Option<AppType>,
    app: Option<String>,
    appType: Option<String>,
    id: String,
) -> Result<bool, String> {
    let app_type = app_type
        .or_else(|| app.as_deref().map(|s| s.into()))
        .or_else(|| appType.as_deref().map(|s| s.into()))
        .unwrap_or(AppType::Claude);

    let mut config = state
        .config
        .lock()
        .map_err(|e| format!("获取锁失败: {}", e))?;

    let manager = config
        .get_manager_mut(&app_type)
        .ok_or_else(|| format!("应用类型不存在: {:?}", app_type))?;

    // 检查是否为当前供应商
    if manager.current == id {
        return Err("不能删除当前正在使用的供应商".to_string());
    }

    // 获取供应商信息
    let provider = manager
        .providers
        .get(&id)
        .ok_or_else(|| format!("供应商不存在: {}", id))?
        .clone();

    // 删除配置文件
    match app_type {
        AppType::Codex => {
            codex_config::delete_codex_provider_config(&id, &provider.name)?;
        }
        AppType::Claude => {
            use crate::config::{delete_file, get_provider_config_path};
            // 兼容历史两种命名：settings-{name}.json 与 settings-{id}.json
            let by_name = get_provider_config_path(&id, Some(&provider.name));
            let by_id = get_provider_config_path(&id, None);
            delete_file(&by_name)?;
            delete_file(&by_id)?;
        }
    }

    // 从管理器删除
    manager.providers.remove(&id);

    // 保存配置
    drop(config); // 释放锁
    state.save()?;

    Ok(true)
}

/// 切换供应商
#[tauri::command]
pub async fn switch_provider(
    state: State<'_, AppState>,
    app_type: Option<AppType>,
    app: Option<String>,
    appType: Option<String>,
    id: String,
) -> Result<bool, String> {
    let app_type = app_type
        .or_else(|| app.as_deref().map(|s| s.into()))
        .or_else(|| appType.as_deref().map(|s| s.into()))
        .unwrap_or(AppType::Claude);

    let mut config = state
        .config
        .lock()
        .map_err(|e| format!("获取锁失败: {}", e))?;

    let manager = config
        .get_manager_mut(&app_type)
        .ok_or_else(|| format!("应用类型不存在: {:?}", app_type))?;

    // 检查供应商是否存在
    let provider = manager
        .providers
        .get(&id)
        .ok_or_else(|| format!("供应商不存在: {}", id))?
        .clone();

    // SSOT 切换：先回填 live 配置到当前供应商，然后从内存写入目标主配置
    match app_type {
        AppType::Codex => {
            use serde_json::Value;

            // 回填：读取 live（auth.json + config.toml）写回当前供应商 settings_config
            if !manager.current.is_empty() {
                let settings = load_settings();
                let auth_path = wsl_env::env_codex_auth_path(&settings)?;
                let config_path = wsl_env::env_codex_config_path(&settings)?;
                if auth_path.exists() {
                    let auth: Value = crate::config::read_json_file(&auth_path)?;
                    let config_str = crate::codex_config::read_codex_config_text_at(&config_path)?;

                    let live = serde_json::json!({
                        "auth": auth,
                        "config": config_str,
                    });

                    if let Some(cur) = manager.providers.get_mut(&manager.current) {
                        cur.settings_config = live;
                    }
                }
            }

            // 切换：从目标供应商 settings_config 写入主配置（Codex 双文件原子+回滚）
            let auth = provider
                .settings_config
                .get("auth")
                .ok_or_else(|| "目标供应商缺少 auth 配置".to_string())?;
            let cfg_text = provider
                .settings_config
                .get("config")
                .and_then(|v| v.as_str());
            let settings = load_settings();
            let auth_path = wsl_env::env_codex_auth_path(&settings)?;
            let cfg_path = wsl_env::env_codex_config_path(&settings)?;
            crate::codex_config::write_codex_live_atomic_at(auth, cfg_text, &auth_path, &cfg_path)?;
        }
        AppType::Claude => {
            use crate::config::{read_json_file, write_json_file};

            let settings = load_settings();
            let settings_path = wsl_env::env_claude_settings_path(&settings)?;

            // 回填：读取 live settings.json 写回当前供应商 settings_config
            if settings_path.exists() && !manager.current.is_empty() {
                if let Ok(live) = read_json_file::<serde_json::Value>(&settings_path) {
                    if let Some(cur) = manager.providers.get_mut(&manager.current) {
                        cur.settings_config = live;
                    }
                }
            }

            // 切换：从目标供应商 settings_config 写入主配置
            if let Some(parent) = settings_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
            }

            // 不做归档，直接写入
            write_json_file(&settings_path, &provider.settings_config)?;
        }
    }

    // 更新当前供应商
    manager.current = id;

    log::info!("成功切换到供应商: {}", provider.name);

    // 保存配置
    drop(config); // 释放锁
    state.save()?;

    Ok(true)
}

/// 导入当前配置为默认供应商
#[tauri::command]
pub async fn import_default_config(
    state: State<'_, AppState>,
    app_type: Option<AppType>,
    app: Option<String>,
    appType: Option<String>,
) -> Result<bool, String> {
    let app_type = app_type
        .or_else(|| app.as_deref().map(|s| s.into()))
        .or_else(|| appType.as_deref().map(|s| s.into()))
        .unwrap_or(AppType::Claude);

    // 仅当 providers 为空时才从 live 导入一条默认项
    {
        let config = state
            .config
            .lock()
            .map_err(|e| format!("获取锁失败: {}", e))?;

        if let Some(manager) = config.get_manager(&app_type) {
            if !manager.get_all_providers().is_empty() {
                return Ok(true);
            }
        }
    }

    // 根据应用类型导入配置
    // 读取当前主配置为默认供应商（不再写入副本文件）
    let settings_config = match app_type {
        AppType::Codex => {
            let settings = load_settings();
            let auth_path = wsl_env::env_codex_auth_path(&settings)?;
            if !auth_path.exists() {
                return Err("Codex 配置文件不存在".to_string());
            }
            let auth: serde_json::Value =
                crate::config::read_json_file::<serde_json::Value>(&auth_path)?;
            let config_path = wsl_env::env_codex_config_path(&settings)?;
            let config_str = crate::codex_config::read_and_validate_config_from_path_at(&config_path)?;
            serde_json::json!({ "auth": auth, "config": config_str })
        }
        AppType::Claude => {
            let settings = load_settings();
            let settings_path = wsl_env::env_claude_settings_path(&settings)?;
            if !settings_path.exists() {
                return Err("Claude Code 配置文件不存在".to_string());
            }
            crate::config::read_json_file::<serde_json::Value>(&settings_path)?
        }
    };

    // 创建默认供应商（仅首次初始化）
    let provider = Provider::with_id(
        "default".to_string(),
        "default".to_string(),
        settings_config,
        None,
    );

    // 添加到管理器
    let mut config = state
        .config
        .lock()
        .map_err(|e| format!("获取锁失败: {}", e))?;

    let manager = config
        .get_manager_mut(&app_type)
        .ok_or_else(|| format!("应用类型不存在: {:?}", app_type))?;

    manager.providers.insert(provider.id.clone(), provider);
    // 设置当前供应商为默认项
    manager.current = "default".to_string();

    // 保存配置
    drop(config); // 释放锁
    state.save()?;

    Ok(true)
}

/// 获取 Claude Code 配置状态
#[tauri::command]
pub async fn get_claude_config_status() -> Result<ConfigStatus, String> {
    // 基于环境返回 Claude 状态
    let settings = load_settings();
    let path = wsl_env::env_claude_settings_path(&settings)?;
    Ok(crate::config::ConfigStatus {
        exists: path.exists(),
        path: path.to_string_lossy().to_string(),
    })
}

/// 获取应用配置状态（通用）
/// 兼容两种参数：`app_type`（推荐）或 `app`（字符串）
#[tauri::command]
pub async fn get_config_status(
    app_type: Option<AppType>,
    app: Option<String>,
    appType: Option<String>,
) -> Result<ConfigStatus, String> {
    let app = app_type
        .or_else(|| app.as_deref().map(|s| s.into()))
        .or_else(|| appType.as_deref().map(|s| s.into()))
        .unwrap_or(AppType::Claude);

    match app {
        AppType::Claude => {
            let settings = load_settings();
            let path = wsl_env::env_claude_settings_path(&settings)?;
            Ok(ConfigStatus { exists: path.exists(), path: path.to_string_lossy().to_string() })
        }
        AppType::Codex => {
            let settings = load_settings();
            let auth_path = wsl_env::env_codex_auth_path(&settings)?;
            let dir = wsl_env::env_codex_dir(&settings)?;
            let exists = auth_path.exists();
            let path = dir.to_string_lossy().to_string();
            Ok(ConfigStatus { exists, path })
        }
    }
}

/// 获取 Claude Code 配置文件路径
#[tauri::command]
pub async fn get_claude_code_config_path() -> Result<String, String> {
    let settings = load_settings();
    let p = wsl_env::env_claude_settings_path(&settings)?;
    Ok(p.to_string_lossy().to_string())
}

/// 打开配置文件夹
/// 兼容两种参数：`app_type`（推荐）或 `app`（字符串）
#[tauri::command]
pub async fn open_config_folder(
    handle: tauri::AppHandle,
    app_type: Option<AppType>,
    app: Option<String>,
    appType: Option<String>,
) -> Result<bool, String> {
    let app_type = app_type
        .or_else(|| app.as_deref().map(|s| s.into()))
        .or_else(|| appType.as_deref().map(|s| s.into()))
        .unwrap_or(AppType::Claude);

    let settings = load_settings();
    let config_dir = match app_type {
        AppType::Claude => wsl_env::env_claude_dir(&settings)?,
        AppType::Codex => wsl_env::env_codex_dir(&settings)?,
    };

    // 确保目录存在
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    // 使用 opener 插件打开文件夹
    handle
        .opener()
        .open_path(config_dir.to_string_lossy().to_string(), None::<String>)
        .map_err(|e| format!("打开文件夹失败: {}", e))?;

    Ok(true)
}

/// 打开外部链接
#[tauri::command]
pub async fn open_external(app: tauri::AppHandle, url: String) -> Result<bool, String> {
    // 规范化 URL，缺少协议时默认加 https://
    let url = if url.starts_with("http://") || url.starts_with("https://") {
        url
    } else {
        format!("https://{}", url)
    };

    // 使用 opener 插件打开链接
    app.opener()
        .open_url(&url, None::<String>)
        .map_err(|e| format!("打开链接失败: {}", e))?;

    Ok(true)
}

/// 获取应用配置文件路径
#[tauri::command]
pub async fn get_app_config_path() -> Result<String, String> {
    use crate::config::get_app_config_path;

    let config_path = get_app_config_path();
    Ok(config_path.to_string_lossy().to_string())
}

/// 打开应用配置文件夹
#[tauri::command]
pub async fn open_app_config_folder(handle: tauri::AppHandle) -> Result<bool, String> {
    use crate::config::get_app_config_dir;

    let config_dir = get_app_config_dir();

    // 确保目录存在
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    // 使用 opener 插件打开文件夹
    handle
        .opener()
        .open_path(config_dir.to_string_lossy().to_string(), None::<String>)
        .map_err(|e| format!("打开文件夹失败: {}", e))?;

    Ok(true)
}

/// 获取设置
#[tauri::command]
pub async fn get_settings(_state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let s = load_settings();
    Ok(serde_json::to_value(s).map_err(|e| format!("序列化设置失败: {}", e))?)
}

/// 保存设置
#[tauri::command]
pub async fn save_settings(
    _state: State<'_, AppState>,
    settings: serde_json::Value,
) -> Result<bool, String> {
    let mut s = load_settings();
    // 按键名覆盖（兼容前端只传 showInTray 的情况）
    if let Some(v) = settings.get("showInTray").and_then(|v| v.as_bool()) {
        s.show_in_tray = v;
    }
    if let Some(v) = settings.get("targetEnv").and_then(|v| v.as_str()) {
        s.target_env = match v.to_lowercase().as_str() {
            "wsl" => crate::settings::TargetEnv::Wsl,
            _ => crate::settings::TargetEnv::Windows,
        };
    }
    if let Some(v) = settings.get("wslDistro").and_then(|v| v.as_str()) {
        s.wsl_distro = if v.trim().is_empty() { None } else { Some(v.to_string()) };
    }
    persist_settings(&s)?;
    Ok(true)
}

/// 检查更新
#[tauri::command]
pub async fn check_for_updates(handle: tauri::AppHandle) -> Result<bool, String> {
    // 打开 GitHub releases 页面
    handle
        .opener()
        .open_url(
            "https://github.com/farion1231/cc-switch/releases",
            None::<String>,
        )
        .map_err(|e| format!("打开更新页面失败: {}", e))?;

    Ok(true)
}

/// 列出已安装的 WSL 发行版
#[tauri::command]
pub async fn list_wsl_distros() -> Result<Vec<String>, String> {
    wsl_env::list_wsl_distros_impl()
}

/// 解析指定发行版的 $HOME（Linux 路径字符串）
#[tauri::command]
pub async fn resolve_wsl_home(distro: String) -> Result<String, String> {
    wsl_env::resolve_wsl_home_impl(&distro)
}
