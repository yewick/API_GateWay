//! 定位各客户端的用户级配置目录。

use super::ClientPaths;
use std::path::{Path, PathBuf};

/// `CODEX_HOME` 优先，其次 `~/.codex`。
///
/// 做成纯函数便于单测：测试里不要改进程环境变量（并行测试会互相干扰）。
pub fn resolve_codex_home(
    env_value: Option<&str>,
    home_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    if let Some(raw) = env_value.map(str::trim).filter(|s| !s.is_empty()) {
        let path = PathBuf::from(raw);
        if !path.is_absolute() {
            // GUI 进程的 cwd 与用户 shell 不同，相对路径会指向意外位置。
            return Err(format!("CODEX_HOME 必须是绝对路径，当前为: {raw}"));
        }
        return Ok(path);
    }

    home_dir
        .map(|home| home.join(".codex"))
        .ok_or_else(|| "无法确定用户主目录（HOME / USERPROFILE）".to_string())
}

pub fn resolve_codex_paths() -> Result<ClientPaths, String> {
    let env_value = std::env::var("CODEX_HOME").ok();
    let home = dirs::home_dir();
    let root = resolve_codex_home(env_value.as_deref(), home.as_deref())?;
    Ok(ClientPaths {
        config: root.join("config.toml"),
        auth: Some(root.join("auth.json")),
        home: root,
    })
}

/// `CLAUDE_CONFIG_DIR` 优先，其次 `~/.claude`。规则与 CODEX_HOME 一致。
pub fn resolve_claude_home(
    env_value: Option<&str>,
    home_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    if let Some(raw) = env_value.map(str::trim).filter(|s| !s.is_empty()) {
        let path = PathBuf::from(raw);
        if !path.is_absolute() {
            return Err(format!("CLAUDE_CONFIG_DIR 必须是绝对路径，当前为: {raw}"));
        }
        return Ok(path);
    }

    home_dir
        .map(|home| home.join(".claude"))
        .ok_or_else(|| "无法确定用户主目录（HOME / USERPROFILE）".to_string())
}

pub fn resolve_claude_paths() -> Result<ClientPaths, String> {
    let env_value = std::env::var("CLAUDE_CONFIG_DIR").ok();
    let home = dirs::home_dir();
    let root = resolve_claude_home(env_value.as_deref(), home.as_deref())?;
    Ok(ClientPaths {
        config: root.join("settings.json"),
        auth: None,
        home: root,
    })
}

/// Claude 桌面端的 user-data-dir：目录下存在 `configLibrary/_meta.json`。
///
/// macOS 同时可能有 `Claude` 与 `Claude-3p`（第三方推理模式），取 `_meta.json`
/// 最近修改的一个，尽量命中当前活跃的安装。
pub fn resolve_claude_desktop_dir() -> Option<PathBuf> {
    let base = dirs::data_dir()?;
    resolve_claude_desktop_dir_in(&base)
}

pub fn resolve_claude_desktop_dir_in(base: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(base).ok()?;
    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !(name == "Claude" || name.starts_with("Claude-")) {
            continue;
        }
        let meta = path.join("configLibrary").join("_meta.json");
        let Ok(metadata) = std::fs::metadata(&meta) else {
            continue;
        };
        let modified = metadata.modified().unwrap_or(std::time::UNIX_EPOCH);
        candidates.push((modified, path));
    }

    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates.into_iter().map(|(_, path)| path).next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn prefers_codex_home() {
        let home = PathBuf::from("/home/u");
        assert_eq!(
            resolve_codex_home(Some("/opt/codex"), Some(&home)).unwrap(),
            PathBuf::from("/opt/codex")
        );
    }

    #[test]
    fn falls_back_to_dot_codex() {
        let home = PathBuf::from("/home/u");
        assert_eq!(
            resolve_codex_home(None, Some(&home)).unwrap(),
            home.join(".codex")
        );
        assert_eq!(
            resolve_codex_home(Some("   "), Some(&home)).unwrap(),
            home.join(".codex")
        );
    }

    #[test]
    fn rejects_relative_codex_home() {
        assert!(resolve_codex_home(Some("./rel"), None).is_err());
    }

    #[test]
    fn errors_without_home() {
        assert!(resolve_codex_home(None, None).is_err());
    }

    #[test]
    fn claude_home_prefers_env_and_rejects_relative() {
        let home = PathBuf::from("/home/u");
        assert_eq!(
            resolve_claude_home(Some("/opt/claude"), Some(&home)).unwrap(),
            PathBuf::from("/opt/claude")
        );
        assert_eq!(
            resolve_claude_home(None, Some(&home)).unwrap(),
            home.join(".claude")
        );
        assert!(resolve_claude_home(Some("./rel"), Some(&home)).is_err());
    }

    #[test]
    fn desktop_dir_picks_latest_meta() {
        let base = std::env::temp_dir().join(format!(
            "yeapi-desktop-paths-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let older = base.join("Claude");
        let newer = base.join("Claude-3p");
        let unrelated = base.join("OtherApp");
        for dir in [&older, &newer, &unrelated] {
            std::fs::create_dir_all(dir.join("configLibrary")).unwrap();
        }
        std::fs::write(older.join("configLibrary/_meta.json"), "{}").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(newer.join("configLibrary/_meta.json"), "{}").unwrap();

        assert_eq!(resolve_claude_desktop_dir_in(&base), Some(newer));

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn desktop_dir_missing_returns_none() {
        let base = std::env::temp_dir().join(format!(
            "yeapi-desktop-paths-empty-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&base).unwrap();
        assert_eq!(resolve_claude_desktop_dir_in(&base), None);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
