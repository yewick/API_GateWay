//! 接入记录、备份与原子写。
//!
//! 记录里不存 API Key 明文，只存 Key id 与 sha256 指纹。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const STORE_FILE: &str = "client_setup.json";
pub const BACKUP_DIR: &str = "client_setup_backups";
const MAX_BACKUPS: usize = 5;

/// 首次接入前用户配置里、我们准备接管的键的原始值。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PreviousValues {
    #[serde(default)]
    pub model_provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub disable_response_storage: Option<bool>,
    /// 首次接入前已存在 `[model_providers.yeapi]` 时，原表的完整 TOML 片段。
    #[serde(default)]
    pub provider_toml: Option<String>,
    #[serde(default)]
    pub model_providers_existed: bool,
    #[serde(default)]
    pub config_existed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRecord {
    pub applied_at: String,
    pub codex_home: String,
    pub config_path: String,
    pub provider_id: String,
    pub base_url: String,
    pub model: String,
    pub api_key_id: String,
    pub key_fingerprint: String,
    #[serde(default)]
    pub previous: PreviousValues,
    #[serde(default)]
    pub backup_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetupStore {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub clients: BTreeMap<String, ClientRecord>,
}

impl Default for SetupStore {
    fn default() -> Self {
        Self {
            version: 1,
            clients: BTreeMap::new(),
        }
    }
}

fn store_path(app_data: &Path) -> PathBuf {
    app_data.join(STORE_FILE)
}

pub fn load_store(app_data: &Path) -> Result<SetupStore, String> {
    let path = store_path(app_data);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(SetupStore::default()),
        Err(err) => return Err(format!("读取接入记录 {} 失败: {err}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(SetupStore::default());
    }
    serde_json::from_str(&text).map_err(|err| {
        format!(
            "接入记录 {} 解析失败: {err}；请修复或删除该文件后重试",
            path.display()
        )
    })
}

pub fn save_store(app_data: &Path, store: &SetupStore) -> Result<(), String> {
    std::fs::create_dir_all(app_data).map_err(|err| format!("创建应用数据目录失败: {err}"))?;
    let bytes =
        serde_json::to_vec_pretty(store).map_err(|err| format!("序列化接入记录失败: {err}"))?;
    atomic_write(&store_path(app_data), &bytes, 0o600)
}

pub fn read_optional(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("读取 {} 失败: {err}", path.display())),
    }
}

pub fn fingerprint(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 原子写：同目录临时文件 → rename。Unix 新文件用给定权限，已有文件保持原权限。
pub fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("无效文件路径: {}", path.display()))?;
    std::fs::create_dir_all(dir)
        .map_err(|err| format!("创建目录 {} 失败: {err}", dir.display()))?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("yeapi");
    let tmp = dir.join(format!(
        ".{file_name}.tmp-{}",
        uuid::Uuid::new_v4().simple()
    ));

    let effective_mode = existing_mode(path).unwrap_or(mode);
    let write_result = (|| -> std::io::Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(effective_mode);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(err) = write_result {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("写入临时文件失败: {err}"));
    }

    if let Err(err) = replace_file(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "替换 {} 失败: {err}；请关闭占用该文件的程序后重试",
            path.display()
        ));
    }
    Ok(())
}

fn replace_file(tmp: &Path, target: &Path) -> std::io::Result<()> {
    match std::fs::rename(tmp, target) {
        Ok(()) => Ok(()),
        Err(err) => {
            #[cfg(windows)]
            {
                // 杀软/索引器可能短暂锁文件，重试一次。
                std::thread::sleep(std::time::Duration::from_millis(50));
                std::fs::rename(tmp, target)
            }
            #[cfg(not(windows))]
            {
                Err(err)
            }
        }
    }
}

#[cfg(unix)]
fn existing_mode(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .ok()
        .map(|meta| meta.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn existing_mode(_path: &Path) -> Option<u32> {
    None
}

/// 备份 `config.toml` 到应用数据目录；只保留最近 `MAX_BACKUPS` 份。
pub fn backup_config(config: &Path, app_data: &Path) -> Result<PathBuf, String> {
    let dir = app_data.join(BACKUP_DIR);
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("创建备份目录 {} 失败: {err}", dir.display()))?;

    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = dir.join(format!("codex-config-{stamp}.toml"));
    std::fs::copy(config, &dest).map_err(|err| format!("备份 {} 失败: {err}", config.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600));
    }

    prune_backups(&dir);
    Ok(dest)
}

fn prune_backups(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut backups: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        })
        .collect();
    if backups.len() <= MAX_BACKUPS {
        return;
    }
    backups.sort();
    let remove_count = backups.len() - MAX_BACKUPS;
    for path in backups.into_iter().take(remove_count) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "yeapi-client-setup-{}",
                uuid::Uuid::new_v4().simple()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn fingerprint_is_stable() {
        assert_eq!(fingerprint("sk-yeapi-x"), fingerprint("sk-yeapi-x"));
        assert_ne!(fingerprint("sk-yeapi-x"), fingerprint("sk-yeapi-y"));
    }

    #[test]
    fn atomic_write_creates_and_replaces() {
        let dir = TempDir::new();
        let file = dir.path().join("nested/config.toml");
        atomic_write(&file, b"a = 1\n", 0o600).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "a = 1\n");
        atomic_write(&file, b"a = 2\n", 0o600).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "a = 2\n");
    }

    #[test]
    fn store_round_trip_and_missing_file() {
        let dir = TempDir::new();
        let store = load_store(dir.path()).unwrap();
        assert!(store.clients.is_empty());

        let mut store = SetupStore::default();
        store.clients.insert(
            "codex".to_string(),
            ClientRecord {
                applied_at: "now".into(),
                codex_home: "/tmp/.codex".into(),
                config_path: "/tmp/.codex/config.toml".into(),
                provider_id: "yeapi".into(),
                base_url: "http://127.0.0.1:8777/v1".into(),
                model: "m".into(),
                api_key_id: "id".into(),
                key_fingerprint: "fp".into(),
                previous: PreviousValues::default(),
                backup_path: None,
            },
        );
        save_store(dir.path(), &store).unwrap();

        let loaded = load_store(dir.path()).unwrap();
        assert_eq!(loaded.clients.len(), 1);
        assert_eq!(loaded.clients["codex"].model, "m");
    }

    #[test]
    fn backup_keeps_latest_five() {
        let dir = TempDir::new();
        let config = dir.path().join("config.toml");
        std::fs::write(&config, "a = 1\n").unwrap();
        for _ in 0..7 {
            backup_config(&config, dir.path()).unwrap();
            // 同一秒内可能重名，稍等以区分时间戳
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let count = std::fs::read_dir(dir.path().join(BACKUP_DIR))
            .unwrap()
            .flatten()
            .filter(|entry| entry.path().extension().and_then(|e| e.to_str()) == Some("toml"))
            .count();
        assert!(count <= MAX_BACKUPS);
    }
}
