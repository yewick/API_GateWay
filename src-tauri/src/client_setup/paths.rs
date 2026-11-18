//! 定位 Codex 的用户级配置目录。

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
        auth: root.join("auth.json"),
        home: root,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let home = PathBuf::from("/home/u");
        assert!(resolve_codex_home(Some("./rel"), Some(&home)).is_err());
    }

    #[test]
    fn errors_without_home() {
        assert!(resolve_codex_home(None, None).is_err());
    }
}
