//! 多源导入：Git 仓库 / 网页 URL / 本地目录。
//!
//! Git 导入使用 `git2`（vendored libgit2/openssl），终端用户无需安装 git；
//! 导入为同步执行（保证闭环跑通），大仓库后台异步化（`kb_tasks`）列为后续。

use std::path::Path;

use serde::Serialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

use super::models::{ImportSourceInput, KbSource};
use super::processor::{self, SourceInfo};
use super::repository::KbRepository;

/// 导入结果摘要。
#[derive(Debug, Clone, Serialize)]
pub struct ImportSummary {
    pub source_id: String,
    pub file_count: i64,
    pub status: String,
    pub error: Option<String>,
}

/// 按 `source_type` 分发到 git / url / local_dir，并回写导入源状态。
pub async fn import_source(
    pool: &SqlitePool,
    kb_id: &str,
    input: ImportSourceInput,
    app: &AppHandle,
) -> Result<ImportSummary, String> {
    let repo = KbRepository::new(pool.clone());
    let now = crate::utils::time::now_iso();

    let source = KbSource {
        id: crate::utils::id::new_id(),
        kb_id: kb_id.to_string(),
        source_type: input.source_type.clone(),
        source_url: input.url.clone().or_else(|| input.repo_url.clone()),
        source_path: input.dir_path.clone(),
        branch: input.branch.clone(),
        status: "processing".to_string(),
        file_count: 0,
        error: None,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    repo.create_source(&source)
        .await
        .map_err(|e| format!("创建导入源失败: {e}"))?;
    let source_id = source.id.clone();

    let result = match input.source_type.as_str() {
        "git" => import_git(pool, kb_id, &input, app).await,
        "url" => import_url(pool, kb_id, &input, app).await,
        "local_dir" => import_local_dir(pool, kb_id, &input, app).await,
        other => Err(format!("不支持的 source_type: {other}")),
    };

    match result {
        Ok(count) => {
            let _ = sqlx::query(
                "UPDATE kb_sources SET status = 'done', file_count = ?, updated_at = ? WHERE id = ?",
            )
            .bind(count)
            .bind(crate::utils::time::now_iso())
            .bind(&source_id)
            .execute(pool)
            .await;
            let _ = app.emit(
                "source-imported",
                serde_json::json!({
                    "kb_id": kb_id,
                    "source_id": source_id,
                    "status": "done",
                    "file_count": count,
                }),
            );
            Ok(ImportSummary {
                source_id,
                file_count: count,
                status: "done".to_string(),
                error: None,
            })
        }
        Err(e) => {
            let _ = sqlx::query(
                "UPDATE kb_sources SET status = 'failed', error = ?, updated_at = ? WHERE id = ?",
            )
            .bind(&e)
            .bind(crate::utils::time::now_iso())
            .bind(&source_id)
            .execute(pool)
            .await;
            Err(format!("导入失败（source {source_id}）: {e}"))
        }
    }
}

/// Git 仓库导入：clone 到临时目录 → 遍历导入 → 清理。
async fn import_git(
    pool: &SqlitePool,
    kb_id: &str,
    input: &ImportSourceInput,
    app: &AppHandle,
) -> Result<i64, String> {
    let repo_url = input.repo_url.as_deref().ok_or("git 导入缺少 repo_url")?;
    let temp_dir = std::env::temp_dir().join(format!("yeapi_git_{}", crate::utils::id::new_id()));
    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("创建临时目录失败: {e}"))?;

    let result = clone_and_import(
        pool,
        kb_id,
        repo_url,
        input.branch.as_deref(),
        input.token.as_deref(),
        &temp_dir,
        app,
    )
    .await;

    // 清理临时目录（成功失败都删）
    let _ = std::fs::remove_dir_all(&temp_dir);
    result
}

/// clone（同步，跑在阻塞线程池）+ 目录遍历导入。
async fn clone_and_import(
    pool: &SqlitePool,
    kb_id: &str,
    repo_url: &str,
    branch: Option<&str>,
    token: Option<&str>,
    dest: &Path,
    app: &AppHandle,
) -> Result<i64, String> {
    let url = auth_url(repo_url, token);
    let dest = dest.to_path_buf();
    let dest_for_walk = dest.clone();
    let branch = branch.map(|s| s.to_string());

    tokio::task::spawn_blocking(move || {
        let mut builder = git2::build::RepoBuilder::new();
        if let Some(b) = branch.as_deref() {
            builder.branch(b);
        }
        builder
            .clone(&url, &dest)
            .map_err(|e| format!("git clone 失败: {e}"))
    })
    .await
    .map_err(|e| format!("git clone 任务失败: {e}"))??;

    let src = SourceInfo {
        source_type: "git".to_string(),
        source_url: Some(repo_url.to_string()),
        source_path: None,
    };
    walk_and_import(pool, kb_id, &dest_for_walk, &src, app).await
}

/// URL 导入：下载单文件后按文件名导入。
async fn import_url(
    pool: &SqlitePool,
    kb_id: &str,
    input: &ImportSourceInput,
    app: &AppHandle,
) -> Result<i64, String> {
    let url = input.url.as_deref().ok_or("url 导入缺少 url")?;
    let resp = reqwest::get(url)
        .await
        .map_err(|e| format!("下载 {url} 失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载 {url} 返回 {}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取 {url} 失败: {e}"))?
        .to_vec();

    let filename = url_to_filename(url);
    let src = SourceInfo {
        source_type: "url".to_string(),
        source_url: Some(url.to_string()),
        source_path: None,
    };
    processor::process_document(pool, kb_id, &filename, &bytes, &src, app).await?;
    Ok(1)
}

/// 本地目录导入：遍历目录逐个文件处理。
async fn import_local_dir(
    pool: &SqlitePool,
    kb_id: &str,
    input: &ImportSourceInput,
    app: &AppHandle,
) -> Result<i64, String> {
    let dir = input.dir_path.as_deref().ok_or("local_dir 导入缺少 dir_path")?;
    let path = Path::new(dir);
    if !path.is_dir() {
        return Err(format!("目录不存在或不是目录: {dir}"));
    }
    let src = SourceInfo {
        source_type: "local_dir".to_string(),
        source_url: None,
        source_path: None,
    };
    walk_and_import(pool, kb_id, path, &src, app).await
}

/// 遍历目录导入：应用知识库的排除/包含过滤，逐个文件 `process_document`。
async fn walk_and_import(
    pool: &SqlitePool,
    kb_id: &str,
    dir: &Path,
    source: &SourceInfo,
    app: &AppHandle,
) -> Result<i64, String> {
    let repo = KbRepository::new(pool.clone());
    let kb = repo
        .get_kb(kb_id)
        .await
        .map_err(|e| format!("读取知识库失败: {e}"))?;

    let mut count = 0i64;
    let mut total = 0i64;
    let mut errors: Vec<String> = Vec::new();

    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !is_hidden(e))
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        // 隐藏文件/目录已在 filter_entry 跳过，这里做知识库排除/包含过滤
        if should_skip(path, &kb) {
            continue;
        }

        total += 1;
        let content =
            std::fs::read(path).map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
        let rel = path
            .strip_prefix(dir)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| name.to_string());
        let src = SourceInfo {
            source_type: source.source_type.clone(),
            source_url: source.source_url.clone(),
            source_path: Some(rel),
        };
        match processor::process_document(pool, kb_id, name, &content, &src, app).await {
            Ok(_) => count += 1,
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    }

    if count == 0 && total > 0 && !errors.is_empty() {
        return Err(format!("全部 {total} 个文件处理失败，首错：{}", errors[0]));
    }
    if !errors.is_empty() {
        tracing::warn!(
            "导入部分文件失败（{}/{}）：{:?}",
            errors.len(),
            total,
            errors.iter().take(5).collect::<Vec<_>>()
        );
    }
    Ok(count)
}

/// 是否隐藏条目（文件名以 `.` 开头）。用于跳过 `.git` / `.svn` / `.DS_Store` 等。
fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

/// 依据知识库配置过滤文件：
/// - `excluded_dirs`（逗号分隔目录名）：命中路径任一分量则跳过；
/// - `excluded_files`（逗号分隔文件名模式，支持 `*` 通配）：命中文件名则跳过；
/// - `included_files`（逗号分隔文件名模式，如 `.rs`、`*.md`）：非空时仅保留命中者。
fn should_skip(path: &Path, kb: &super::models::KbKnowledgeBase) -> bool {
    let dirs: Vec<&str> = kb
        .excluded_dirs
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    for comp in path.components() {
        if let Some(name) = comp.as_os_str().to_str() {
            if dirs.iter().any(|d| *d == name) {
                return true;
            }
        }
    }

    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let excl: Vec<&str> = kb
        .excluded_files
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if excl.iter().any(|f| pattern_matches(f, name)) {
        return true;
    }

    let incl: Vec<&str> = kb
        .included_files
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if !incl.is_empty() && !incl.iter().any(|f| pattern_matches(f, name)) {
        return true;
    }

    false
}

/// 文件名匹配：支持单个 `*` 通配（`*.lock` → 以 `.lock` 结尾、`test*.rs` → `test` 开头且
/// `.rs` 结尾、`*lock` → 以 `lock` 结尾）；无 `*` 时按子串匹配。
fn pattern_matches(pattern: &str, name: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return false;
    }
    match pattern.find('*') {
        Some(star) => {
            let (prefix, suffix) = pattern.split_at(star);
            let suffix = &suffix[1..]; // 去掉第一个 '*'
            if suffix.contains('*') {
                // 多 '*'：退化为去星子串匹配（少见场景，够用）
                name.contains(pattern.trim_matches('*'))
            } else {
                name.starts_with(prefix) && name.ends_with(suffix)
            }
        }
        None => name.contains(pattern),
    }
}

/// 把 token 拼进 HTTPS URL（`https://x-access-token:{token}@host/...`）。
fn auth_url(repo_url: &str, token: Option<&str>) -> String {
    match token {
        Some(t) if repo_url.starts_with("https://") => {
            repo_url.replacen("https://", &format!("https://x-access-token:{t}@"), 1)
        }
        _ => repo_url.to_string(),
    }
}

/// URL → 文件名（取路径末段，剥掉 query）。
fn url_to_filename(url: &str) -> String {
    let path = url.split('?').next().unwrap_or(url);
    let name = path.rsplit('/').next().unwrap_or_default();
    if name.is_empty() {
        "downloaded".to_string()
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_to_filename() {
        assert_eq!(url_to_filename("https://x.com/a/b.md"), "b.md");
        assert_eq!(url_to_filename("https://x.com/c.md?v=1"), "c.md");
        assert_eq!(url_to_filename("https://x.com/"), "downloaded");
    }

    #[test]
    fn test_auth_url() {
        assert_eq!(
            auth_url("https://github.com/a/b.git", Some("tok")),
            "https://x-access-token:tok@github.com/a/b.git"
        );
        // 非 https 或无 token 原样返回
        assert_eq!(auth_url("git@github.com:a/b.git", Some("tok")), "git@github.com:a/b.git");
        assert_eq!(auth_url("https://github.com/a/b.git", None), "https://github.com/a/b.git");
    }

    #[test]
    fn test_pattern_matches() {
        // 无 '*' → 子串
        assert!(pattern_matches("LICENSE", "LICENSE"));
        assert!(pattern_matches(".rs", "main.rs"));
        assert!(!pattern_matches(".rs", "main.py"));
        // 单 '*' → 前缀 + 后缀
        assert!(pattern_matches("*.lock", "Cargo.lock"));
        assert!(pattern_matches("test*.rs", "test_foo.rs"));
        assert!(!pattern_matches("test*.rs", "foo.rs"));
        assert!(pattern_matches("*lock", "Cargo.lock"));
        // 空串不匹配
        assert!(!pattern_matches("", "anything"));
    }

    #[test]
    fn test_should_skip() {
        use super::super::models::KbKnowledgeBase;
        let kb = KbKnowledgeBase {
            id: "k".into(),
            name: "k".into(),
            description: None,
            status: 1,
            doc_count: 0,
            chunk_count: 0,
            total_tokens: 0,
            embedding_model: None,
            embedding_channel_id: None,
            mcp_enabled: 1,
            chunk_size: 512,
            chunk_overlap: 64,
            excluded_dirs: "target,node_modules".into(),
            excluded_files: "*.lock,LICENSE".into(),
            included_files: "".into(),
            embedding_dim: 0,
            index_status: "none".into(),
            created_at: "".into(),
            updated_at: "".into(),
        };
        assert!(should_skip(Path::new("a/target/x.rs"), &kb));
        assert!(should_skip(Path::new("a/Cargo.lock"), &kb));
        assert!(should_skip(Path::new("LICENSE"), &kb));
        assert!(!should_skip(Path::new("src/main.rs"), &kb));
    }
}
