//! 接入记录、备份与原子写。
//!
//! 记录里不存 API Key 明文，只存 Key id、sha256 指纹与备份文件路径。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const STORE_FILE: &str = "client_setup.json";
pub const BACKUP_DIR: &str = "client_setup_backups";
const MAX_BACKUPS: usize = 5;

/// 首次接入前用户配置里、我们准备接管的键的原始值（Codex）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PreviousValues {
    #[serde(default)]
    pub model_provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub disable_response_storage: Option<bool>,
    #[serde(default)]
    pub model_catalog_json: Option<String>,
    /// 首次接入前已存在 `[model_providers.yeapi]` 时，原表的完整 TOML 片段。
    #[serde(default)]
    pub provider_toml: Option<String>,
    #[serde(default)]
    pub model_providers_existed: bool,
    #[serde(default)]
    pub config_existed: bool,
}

/// Claude Code `settings.json` 中受管 env 键的接入前快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaudePreviousValues {
    #[serde(default)]
    pub env: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub env_existed: bool,
    #[serde(default)]
    pub settings_existed: bool,
}

/// Claude 桌面端第三方推理档案的接入信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeDesktopRecord {
    pub profile_id: String,
    pub profile_name: String,
    pub config_dir: String,
    pub meta_path: String,
    pub profile_path: String,
    #[serde(default)]
    pub previous_applied_id: Option<String>,
    #[serde(default)]
    pub previous_meta_existed: bool,
    #[serde(default)]
    pub previous_meta_backup: Option<String>,
    #[serde(default)]
    pub previous_profile_backup: Option<String>,
    /// 接入前 `claude.desktop_aliases` 的值（网关模型路由别名）。
    #[serde(default)]
    pub previous_aliases: Option<serde_json::Value>,
    #[serde(default)]
    pub created_profile: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeRecordData {
    pub previous: ClaudePreviousValues,
    pub applied_models: super::ClaudeModelAliases,
    /// 本次接入是否写入了 CLI 的 settings.json。
    #[serde(default = "default_true")]
    pub cli_applied: bool,
    #[serde(default)]
    pub desktop: Option<ClaudeDesktopRecord>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRecord {
    pub applied_at: String,
    #[serde(alias = "codex_home")]
    pub config_home: String,
    pub config_path: String,
    pub base_url: String,
    pub model: String,
    pub api_key_id: String,
    pub key_fingerprint: String,
    #[serde(default)]
    pub provider_id: String,
    /// 仅 Codex 使用。
    #[serde(default)]
    pub previous: PreviousValues,
    /// 仅 Claude Code 使用。
    #[serde(default)]
    pub claude: Option<ClaudeRecordData>,
    #[serde(default)]
    pub backup_path: Option<String>,
    /// 接入时 Key 是否由向导新建（可安全自动轮换）。
    #[serde(default)]
    pub key_managed: bool,
    #[serde(default)]
    pub key_name: Option<String>,
    #[serde(default)]
    pub key_allowed_models: Vec<String>,
    #[serde(default)]
    pub key_quota_limit: Option<i64>,
    /// 用户在 YeAPI 里主动删除了该 Key：绝不自动复活。
    #[serde(default)]
    pub key_revoked: bool,
    #[serde(default)]
    pub last_reconciled_at: Option<String>,
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
            version: 2,
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
    atomic_write_secret(&store_path(app_data), &bytes)
}

/// 用户在 YeAPI 里删除 Key 后留下墓碑：自检不会自动轮换被主动撤销的 Key。
pub fn mark_key_revoked(app: &tauri::AppHandle, key_id: &str) -> Result<(), String> {
    use tauri::Manager;
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("无法获取应用数据目录: {err}"))?;
    let mut store = load_store(&app_data)?;
    let mut changed = false;
    for record in store.clients.values_mut() {
        if record.api_key_id == key_id {
            record.key_revoked = true;
            changed = true;
        }
    }
    if changed {
        save_store(&app_data, &store)?;
    }
    Ok(())
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
    atomic_write_impl(path, bytes, mode, false)
}

/// 写入含凭据的文件：无论原文件权限如何，都强制 0600。
pub fn atomic_write_secret(path: &Path, bytes: &[u8]) -> Result<(), String> {
    atomic_write_impl(path, bytes, 0o600, true)
}

fn atomic_write_impl(path: &Path, bytes: &[u8], mode: u32, force_mode: bool) -> Result<(), String> {
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

    let effective_mode = if force_mode {
        mode
    } else {
        existing_mode(path).unwrap_or(mode)
    };
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

    #[cfg(unix)]
    if force_mode {
        // OpenOptions::mode 会被 umask 削弱，这里再显式收紧一次。
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode));
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

/// 备份配置文件到应用数据目录；按 label + 扩展名分别保留最近 `MAX_BACKUPS` 份。
///
/// `label` 例如 `codex-config` / `claude-settings` / `claude-desktop-meta`。
pub fn backup_file(source: &Path, app_data: &Path, label: &str) -> Result<PathBuf, String> {
    let dir = app_data.join(BACKUP_DIR);
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("创建备份目录 {} 失败: {err}", dir.display()))?;

    let ext = source
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_else(|| "bak".to_string());
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f");

    let mut dest = dir.join(format!("{label}-{stamp}.{ext}"));
    let mut counter = 1;
    while dest.exists() {
        dest = dir.join(format!("{label}-{stamp}-{counter}.{ext}"));
        counter += 1;
    }

    std::fs::copy(source, &dest).map_err(|err| format!("备份 {} 失败: {err}", source.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600));
    }

    prune_backups(&dir, label, &ext);
    Ok(dest)
}

fn prune_backups(dir: &Path, label: &str, ext: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let prefix = format!("{label}-");
    let suffix = format!(".{ext}");
    let mut backups: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(&suffix))
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

    fn codex_record() -> ClientRecord {
        ClientRecord {
            applied_at: "now".into(),
            config_home: "/tmp/.codex".into(),
            config_path: "/tmp/.codex/config.toml".into(),
            base_url: "http://127.0.0.1:8777/v1".into(),
            model: "m".into(),
            api_key_id: "id".into(),
            key_fingerprint: "fp".into(),
            provider_id: "yeapi".into(),
            previous: PreviousValues::default(),
            claude: None,
            backup_path: None,
            key_managed: false,
            key_name: None,
            key_allowed_models: Vec::new(),
            key_quota_limit: None,
            key_revoked: false,
            last_reconciled_at: None,
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

    #[cfg(unix)]
    #[test]
    fn atomic_write_secret_forces_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new();
        let file = dir.path().join("settings.json");
        std::fs::write(&file, "{}").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();

        atomic_write_secret(&file, b"{\"a\":1}").unwrap();
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn store_round_trip_and_missing_file() {
        let dir = TempDir::new();
        let store = load_store(dir.path()).unwrap();
        assert!(store.clients.is_empty());

        let mut store = SetupStore::default();
        store.clients.insert("codex".to_string(), codex_record());
        save_store(dir.path(), &store).unwrap();

        let loaded = load_store(dir.path()).unwrap();
        assert_eq!(loaded.clients.len(), 1);
        assert_eq!(loaded.clients["codex"].model, "m");
    }

    #[test]
    fn store_reads_legacy_codex_home_field() {
        let dir = TempDir::new();
        let legacy = serde_json::json!({
            "version": 1,
            "clients": {
                "codex": {
                    "applied_at": "now",
                    "codex_home": "/tmp/.codex",
                    "config_path": "/tmp/.codex/config.toml",
                    "provider_id": "yeapi",
                    "base_url": "http://127.0.0.1:8777/v1",
                    "model": "m",
                    "api_key_id": "id",
                    "key_fingerprint": "fp",
                    "previous": {},
                    "backup_path": null
                }
            }
        });
        std::fs::write(
            dir.path().join(STORE_FILE),
            serde_json::to_vec_pretty(&legacy).unwrap(),
        )
        .unwrap();

        let loaded = load_store(dir.path()).unwrap();
        assert_eq!(loaded.clients["codex"].config_home, "/tmp/.codex");
        assert!(loaded.clients["codex"].claude.is_none());
    }

    #[test]
    fn backup_keeps_latest_five_per_label() {
        let dir = TempDir::new();
        let config = dir.path().join("config.toml");
        std::fs::write(&config, "a = 1\n").unwrap();
        for _ in 0..7 {
            backup_file(&config, dir.path(), "codex-config").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let count = std::fs::read_dir(dir.path().join(BACKUP_DIR))
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry.file_name().to_str().is_some_and(|name| {
                    name.starts_with("codex-config-") && name.ends_with(".toml")
                })
            })
            .count();
        assert!(count <= MAX_BACKUPS);
    }

    #[test]
    fn backup_labels_do_not_crowd_each_other() {
        let dir = TempDir::new();
        let config = dir.path().join("config.toml");
        let settings = dir.path().join("settings.json");
        std::fs::write(&config, "a = 1\n").unwrap();
        std::fs::write(&settings, "{}").unwrap();
        for _ in 0..5 {
            backup_file(&config, dir.path(), "codex-config").unwrap();
            backup_file(&settings, dir.path(), "claude-settings").unwrap();
        }
        let backups = std::fs::read_dir(dir.path().join(BACKUP_DIR))
            .unwrap()
            .count();
        assert_eq!(backups, 10);
    }
}
