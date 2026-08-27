//! MinerU 云服务后端（https://mineru.net）。
//!
//! 面向复杂 PDF（扫描页 / 复杂表格 / 多栏 / 图片文字 / 公式 / 非标准阅读顺序）的云解析。
//! 两种调用方式（骨架均为「提交 → 上传 → 轮询 → 下载」）：
//! - Agent 轻量 API（无 token，按 IP 限流，≤10MB/≤20 页）：返回 `markdown_url` 直接下载 md。
//! - Precise API（需 token，≤200MB/≤200 页）：返回 `full_zip_url`，zip 内含 `full.md` + 结构化 JSON。
//!
//! 配置（[`MinerUConfig::resolve`]）：store（settings.json 的 `knowledge.mineru.*`）优先，
//! 环境变量 `YEAPI_MINERU_TOKEN`（有值→Precise，无→Agent）、`YEAPI_MINERU_BASE_URL`（默认
//! https://mineru.net）、`YEAPI_MINERU_MODEL`（Precise 用，默认 pipeline）次之，便于无前端时临时配置。

use std::error::Error;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;

use super::pdf::{ParseProgress, PdfExtractor};

const DEFAULT_BASE_URL: &str = "https://mineru.net";
const POLL_INTERVAL: Duration = Duration::from_secs(3);
const MAX_POLLS: usize = 100; // ≈ 5 分钟
const REQ_TIMEOUT: Duration = Duration::from_secs(180);
/// 结果下载重试次数（应对 CDN 偶发抖动）。
const DOWNLOAD_RETRIES: usize = 3;
/// 重试退避基数（第 n 次重试延迟 n 秒）。
const RETRY_BASE_DELAY: Duration = Duration::from_secs(1);

/// MinerU 云服务后端（配置由构造时注入，见 [`MinerUConfig::resolve`]）。
pub struct MinerUExtractor {
    cfg: MinerUConfig,
}

/// MinerU 配置：store 优先 → 环境变量 → 默认值。
#[derive(Debug, Clone)]
pub struct MinerUConfig {
    /// 有值 → Precise API；无 → Agent 轻量 API
    pub token: Option<String>,
    pub base_url: String,
    pub model_version: String,
}

impl MinerUConfig {
    /// 解析配置：store 键 `knowledge.mineru.token / base_url / model` 优先（前端可调），
    /// `YEAPI_MINERU_TOKEN / BASE_URL / MODEL` 环境变量次之（无前端时临时配置），最后默认值。
    pub fn resolve(app: Option<&tauri::AppHandle>) -> Self {
        // store 读取（无 AppHandle 或 store 不可用时静默跳过）
        let mut token: Option<String> = None;
        let mut base_url: Option<String> = None;
        let mut model_version: Option<String> = None;
        if let Some(app) = app {
            use tauri_plugin_store::StoreExt;
            if let Ok(store) = app.store("settings.json") {
                let get = |key: &str| -> Option<String> {
                    store
                        .get(key)
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                        .filter(|s| !s.trim().is_empty())
                };
                token = get("knowledge.mineru.token");
                base_url = get("knowledge.mineru.base_url");
                model_version = get("knowledge.mineru.model");
            }
        }

        // 环境变量兜底
        let env = |name: &str| std::env::var(name).ok().filter(|s| !s.trim().is_empty());
        let token = token.or_else(|| env("YEAPI_MINERU_TOKEN"));
        let base_url = base_url
            .or_else(|| env("YEAPI_MINERU_BASE_URL"))
            .map(|s| s.trim_end_matches('/').to_string())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let model_version = model_version
            .or_else(|| env("YEAPI_MINERU_MODEL"))
            .unwrap_or_else(|| "pipeline".to_string());
        Self {
            token,
            base_url,
            model_version,
        }
    }
}

impl MinerUExtractor {
    pub fn new(cfg: MinerUConfig) -> Self {
        Self { cfg }
    }
}

#[async_trait]
impl PdfExtractor for MinerUExtractor {
    async fn extract(
        &self,
        filename: &str,
        content: &[u8],
        progress: Option<UnboundedSender<ParseProgress>>,
    ) -> Result<String, String> {
        let cfg = &self.cfg;
        let client = reqwest::Client::builder()
            .timeout(REQ_TIMEOUT)
            .build()
            .map_err(|e| format!("构造 MinerU HTTP 客户端失败: {e}"))?;

        match &cfg.token {
            Some(_) => Self::extract_precise(&client, cfg, filename, content, &progress).await,
            None => Self::extract_agent(&client, cfg, filename, content, &progress).await,
        }
    }
}

impl MinerUExtractor {
    /// Agent 轻量 API（无 token）：提交 → 上传 → 轮询 → 下载 markdown。
    async fn extract_agent(
        client: &reqwest::Client,
        cfg: &MinerUConfig,
        filename: &str,
        content: &[u8],
        progress: &Option<UnboundedSender<ParseProgress>>,
    ) -> Result<String, String> {
        send_progress(progress, "submitting", 0, 0);

        // 1. 提交：拿 task_id + OSS 签名上传 URL
        let submit_url = format!("{}/api/v1/agent/parse/file", cfg.base_url);
        let resp = client
            .post(&submit_url)
            .json(&serde_json::json!({ "file_name": filename }))
            .send()
            .await
            .map_err(|e| format!("MinerU 提交失败: {e}"))?;
        let text = read_body(resp).await?;
        let (task_id, upload_url) = parse_agent_submit(&text)?;

        // 2. 上传原始字节（PUT 签名 URL）
        send_progress(progress, "uploading", 0, 0);
        let put_resp = client
            .put(&upload_url)
            .body(content.to_vec())
            .send()
            .await
            .map_err(|e| format!("MinerU 上传失败: {e}"))?;
        if !put_resp.status().is_success() {
            return Err(format!("MinerU 上传返回 {}", put_resp.status()));
        }

        // 3. 轮询到完成
        let markdown_url = Self::poll_agent(client, cfg, &task_id, progress).await?;

        // 4. 下载 markdown
        send_progress(progress, "downloading", 0, 0);
        let md = download_text(client, &markdown_url).await?;
        send_progress(progress, "done", 0, 0);
        Ok(md)
    }

    /// Precise API（需 token）：批量申请 → OSS 签名上传 → 轮询（带页数进度）→ 下载 zip 取 `full.md`。
    async fn extract_precise(
        client: &reqwest::Client,
        cfg: &MinerUConfig,
        filename: &str,
        content: &[u8],
        progress: &Option<UnboundedSender<ParseProgress>>,
    ) -> Result<String, String> {
        send_progress(progress, "submitting", 0, 0);
        let token = cfg.token.as_deref().unwrap_or_default();

        // 1. 批量上传申请（单文件）
        let submit_url = format!("{}/api/v4/file-urls/batch", cfg.base_url);
        let data_id = crate::utils::id::new_id();
        let resp = client
            .post(&submit_url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "files": [{ "name": filename, "data_id": data_id }],
                "model_version": cfg.model_version,
            }))
            .send()
            .await
            .map_err(|e| format!("MinerU 提交失败: {e}"))?;
        let text = read_body(resp).await?;
        let (batch_id, upload_url) = parse_precise_submit(&text)?;

        // 2. 上传原始字节（无 Content-Type）
        send_progress(progress, "uploading", 0, 0);
        let put_resp = client
            .put(&upload_url)
            .body(content.to_vec())
            .send()
            .await
            .map_err(|e| format!("MinerU 上传失败: {e}"))?;
        if !put_resp.status().is_success() {
            return Err(format!("MinerU 上传返回 {}", put_resp.status()));
        }

        // 3. 轮询批量结果（带页数进度）
        let zip_url = Self::poll_precise(client, cfg, &batch_id, progress).await?;

        // 4. 下载 zip → 解压 full.md
        send_progress(progress, "downloading", 0, 0);
        let zip_bytes = download_bytes(client, &zip_url).await?;
        let md = extract_full_md_from_zip(&zip_bytes)?;
        send_progress(progress, "done", 0, 0);
        Ok(md)
    }

    async fn poll_agent(
        client: &reqwest::Client,
        cfg: &MinerUConfig,
        task_id: &str,
        progress: &Option<UnboundedSender<ParseProgress>>,
    ) -> Result<String, String> {
        let url = format!("{}/api/v1/agent/parse/{task_id}", cfg.base_url);
        for _ in 0..MAX_POLLS {
            tokio::time::sleep(POLL_INTERVAL).await;
            let resp = client
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("MinerU 查询失败: {e}"))?;
            let text = read_body(resp).await?;
            let out = parse_agent_poll(&text)?;
            match out.state.as_str() {
                "done" => {
                    send_progress(progress, "done", 0, 0);
                    return out
                        .markdown_url
                        .ok_or_else(|| "MinerU 完成但响应缺少 markdown_url".to_string());
                }
                "failed" => {
                    return Err(format!(
                        "MinerU 解析失败: {}",
                        out.err_msg.unwrap_or_default()
                    ))
                }
                // Agent 无页数进度 → total=0（不定进度）
                _ => send_progress(progress, "parsing", 0, 0),
            }
        }
        Err(format!("MinerU 解析超时（超过 {MAX_POLLS} 轮轮询）"))
    }

    async fn poll_precise(
        client: &reqwest::Client,
        cfg: &MinerUConfig,
        batch_id: &str,
        progress: &Option<UnboundedSender<ParseProgress>>,
    ) -> Result<String, String> {
        let url = format!("{}/api/v4/extract-results/batch/{batch_id}", cfg.base_url);
        let token = cfg.token.as_deref().unwrap_or_default();
        for _ in 0..MAX_POLLS {
            tokio::time::sleep(POLL_INTERVAL).await;
            let resp = client
                .get(&url)
                .header("Authorization", format!("Bearer {token}"))
                .send()
                .await
                .map_err(|e| format!("MinerU 查询失败: {e}"))?;
            let text = read_body(resp).await?;
            let out = parse_precise_poll(&text)?;
            match out.state.as_str() {
                "done" => {
                    send_progress(progress, "done", 0, 0);
                    return out
                        .full_zip_url
                        .ok_or_else(|| "MinerU 完成但响应缺少 full_zip_url".to_string());
                }
                "failed" => {
                    return Err(format!(
                        "MinerU 解析失败: {}",
                        out.err_msg.unwrap_or_default()
                    ))
                }
                // 有页数进度 → 真实进度条
                _ => send_progress(progress, "parsing", out.extracted_pages, out.total_pages),
            }
        }
        Err(format!("MinerU 解析超时（超过 {MAX_POLLS} 轮轮询）"))
    }
}

// ---------------------------------------------------------------------------
// HTTP 辅助
// ---------------------------------------------------------------------------

/// 读取响应体：非 2xx 直接报错，否则返回文本。
async fn read_body(resp: reqwest::Response) -> Result<String, String> {
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("MinerU 响应读取失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("MinerU 请求返回 {status}: {}", truncate(&text, 300)));
    }
    Ok(text)
}

async fn download_text(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let mut last_err = String::new();
    for attempt in 0..DOWNLOAD_RETRIES {
        match try_download_text(client, url).await {
            Ok(t) => return Ok(t),
            Err(e) => {
                last_err = e;
                if attempt + 1 < DOWNLOAD_RETRIES {
                    tokio::time::sleep(RETRY_BASE_DELAY * (attempt as u32 + 1)).await;
                }
            }
        }
    }
    Err(last_err)
}

async fn try_download_text(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载 {url} 失败: {}", describe_http_error(&e)))?;
    read_body(resp).await
}

async fn download_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let mut last_err = String::new();
    for attempt in 0..DOWNLOAD_RETRIES {
        match try_download_bytes(client, url).await {
            Ok(b) => return Ok(b),
            Err(e) => {
                last_err = e;
                if attempt + 1 < DOWNLOAD_RETRIES {
                    tokio::time::sleep(RETRY_BASE_DELAY * (attempt as u32 + 1)).await;
                }
            }
        }
    }
    Err(last_err)
}

async fn try_download_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载 {url} 失败: {}", describe_http_error(&e)))?;
    if !resp.status().is_success() {
        return Err(format!("下载 {url} 返回 {}", resp.status()));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("读取 {url} 失败: {}", describe_http_error(&e)))
}

/// 展开 reqwest 错误根因（默认只显示 "error sending request for url"）。
fn describe_http_error(e: &reqwest::Error) -> String {
    let mut hints: Vec<String> = Vec::new();
    if e.is_timeout() {
        hints.push("请求超时".to_string());
    }
    if e.is_connect() {
        hints.push("连接失败（DNS/网络不可达/代理未生效）".to_string());
    }
    let mut src = e.source();
    let mut seen = 0;
    while let Some(s) = src {
        if seen >= 3 {
            break;
        }
        let msg = s.to_string();
        if !msg.is_empty() && !hints.contains(&msg) {
            hints.push(msg);
        }
        src = s.source();
        seen += 1;
    }
    if hints.is_empty() {
        e.to_string()
    } else {
        hints.join("；")
    }
}

fn send_progress(progress: &Option<UnboundedSender<ParseProgress>>, stage: &str, done: u64, total: u64) {
    if let Some(tx) = progress {
        let _ = tx.send(ParseProgress {
            stage: stage.to_string(),
            done,
            total,
        });
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t}…")
    }
}

// ---------------------------------------------------------------------------
// 响应解析（纯函数，可单测）
// ---------------------------------------------------------------------------

/// 轮询结果（Agent 与 Precise 共用；仅一条流程用到各自的字段）。
#[derive(Debug, PartialEq)]
struct PollOutcome {
    state: String,
    markdown_url: Option<String>,
    full_zip_url: Option<String>,
    err_msg: Option<String>,
    extracted_pages: u64,
    total_pages: u64,
}

fn parse_json(text: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(text).map_err(|e| format!("MinerU 响应非 JSON: {e}"))
}

/// 解析 Agent 提交响应：`data.task_id` + `data.file_url`。
fn parse_agent_submit(text: &str) -> Result<(String, String), String> {
    let v = parse_json(text)?;
    check_code(&v)?;
    let data = v.get("data").ok_or("MinerU 提交响应缺少 data")?;
    let task_id = data
        .get("task_id")
        .and_then(|t| t.as_str())
        .ok_or("MinerU 提交响应缺少 task_id")?
        .to_string();
    let file_url = data
        .get("file_url")
        .and_then(|u| u.as_str())
        .ok_or("MinerU 提交响应缺少 file_url")?
        .to_string();
    Ok((task_id, file_url))
}

/// 解析 Agent 轮询响应：`data.state` / `data.markdown_url` / `data.err_msg`。
fn parse_agent_poll(text: &str) -> Result<PollOutcome, String> {
    let v = parse_json(text)?;
    let data = v.get("data").ok_or("MinerU 查询响应缺少 data")?;
    Ok(PollOutcome {
        state: data
            .get("state")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
        markdown_url: data
            .get("markdown_url")
            .and_then(|u| u.as_str())
            .map(|s| s.to_string()),
        full_zip_url: None,
        err_msg: data
            .get("err_msg")
            .or_else(|| data.get("error"))
            .and_then(|m| m.as_str())
            .map(|s| s.to_string()),
        extracted_pages: 0,
        total_pages: 0,
    })
}

/// 解析 Precise 批量申请响应：`data.batch_id` + `data.file_urls[0]`。
fn parse_precise_submit(text: &str) -> Result<(String, String), String> {
    let v = parse_json(text)?;
    check_code(&v)?;
    let data = v.get("data").ok_or("MinerU 提交响应缺少 data")?;
    let batch_id = data
        .get("batch_id")
        .and_then(|b| b.as_str())
        .ok_or("MinerU 提交响应缺少 batch_id")?
        .to_string();
    let upload_url = data
        .get("file_urls")
        .and_then(|u| u.as_array())
        .and_then(|arr| arr.first())
        .and_then(|u| u.as_str())
        .ok_or("MinerU 提交响应缺少 file_urls")?
        .to_string();
    Ok((batch_id, upload_url))
}

/// 解析 Precise 批量结果轮询：`data.extract_result[0]`（或 `extract_results`），
/// 取 `state` / `full_zip_url` / `err_msg` / `extract_progress`（页数）。
fn parse_precise_poll(text: &str) -> Result<PollOutcome, String> {
    let v = parse_json(text)?;
    let data = v.get("data").ok_or("MinerU 查询响应缺少 data")?;
    let item = data
        .get("extract_result")
        .or_else(|| data.get("extract_results"))
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.first())
        .ok_or("MinerU 查询响应缺少 extract_result 数组")?;

    let (extracted_pages, total_pages) = item
        .get("extract_progress")
        .map(|p| {
            let done = p
                .get("extracted_pages")
                .or_else(|| p.get("done"))
                .and_then(|n| n.as_u64())
                .unwrap_or(0);
            let total = p
                .get("total_pages")
                .or_else(|| p.get("total"))
                .and_then(|n| n.as_u64())
                .unwrap_or(0);
            (done, total)
        })
        .unwrap_or((0, 0));

    Ok(PollOutcome {
        state: item
            .get("state")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
        markdown_url: None,
        full_zip_url: item
            .get("full_zip_url")
            .and_then(|u| u.as_str())
            .map(|s| s.to_string()),
        err_msg: item
            .get("err_msg")
            .or_else(|| item.get("error"))
            .and_then(|m| m.as_str())
            .map(|s| s.to_string()),
        extracted_pages,
        total_pages,
    })
}

/// 若响应体含非 0 的 `code`，返回错误。
fn check_code(v: &serde_json::Value) -> Result<(), String> {
    if let Some(code) = v.get("code").and_then(|c| c.as_i64()) {
        if code != 0 {
            let msg = v
                .get("msg")
                .or_else(|| v.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("未知错误");
            return Err(format!("MinerU 返回错误（code={code}）: {msg}"));
        }
    }
    Ok(())
}

/// 从 MinerU 结果 zip 中读取 `full.md`（缺省回退首个 `*.md`）。
fn extract_full_md_from_zip(bytes: &[u8]) -> Result<String, String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("解压 MinerU 结果 zip 失败: {e}"))?;

    let mut fallback: Option<String> = None;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("读取 zip 条目失败: {e}"))?;
        let name = file.name().to_string();
        let is_md = name == "full.md" || name.ends_with(".md");
        if !is_md {
            continue;
        }
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut buf)
            .map_err(|e| format!("读取 {name} 失败: {e}"))?;
        let text = String::from_utf8_lossy(&buf).to_string();
        if name == "full.md" {
            return Ok(text);
        }
        fallback.get_or_insert(text);
    }
    fallback.ok_or_else(|| "MinerU 结果 zip 中未找到 full.md".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut cursor);
            let opts = zip::write::SimpleFileOptions::default();
            for (name, data) in entries {
                zw.start_file(*name, opts).unwrap();
                zw.write_all(data).unwrap();
            }
            zw.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn test_parse_agent_submit() {
        let (task, url) =
            parse_agent_submit(r#"{"code":0,"data":{"task_id":"t1","file_url":"https://oss/x"}}"#)
                .unwrap();
        assert_eq!(task, "t1");
        assert_eq!(url, "https://oss/x");
    }

    #[test]
    fn test_parse_agent_submit_error_code() {
        assert!(parse_agent_submit(r#"{"code":-30001,"msg":"文件过大"}"#).is_err());
    }

    #[test]
    fn test_parse_agent_poll_done() {
        let out = parse_agent_poll(
            r#"{"code":0,"data":{"state":"done","markdown_url":"https://cdn/full.md"}}"#,
        )
        .unwrap();
        assert_eq!(out.state, "done");
        assert_eq!(out.markdown_url.as_deref(), Some("https://cdn/full.md"));
    }

    #[test]
    fn test_parse_agent_poll_failed() {
        let out =
            parse_agent_poll(r#"{"code":0,"data":{"state":"failed","err_msg":"boom"}}"#).unwrap();
        assert_eq!(out.state, "failed");
        assert_eq!(out.err_msg.as_deref(), Some("boom"));
    }

    #[test]
    fn test_parse_precise_submit() {
        let (batch, url) = parse_precise_submit(
            r#"{"code":0,"data":{"batch_id":"b1","file_urls":["https://oss/presign"]}}"#,
        )
        .unwrap();
        assert_eq!(batch, "b1");
        assert_eq!(url, "https://oss/presign");
    }

    #[test]
    fn test_parse_precise_poll_progress() {
        let out = parse_precise_poll(
            r#"{"code":0,"data":{"extract_result":[{"state":"running","extract_progress":{"extracted_pages":3,"total_pages":10}}]}}"#,
        )
        .unwrap();
        assert_eq!(out.state, "running");
        assert_eq!(out.extracted_pages, 3);
        assert_eq!(out.total_pages, 10);
    }

    #[test]
    fn test_parse_precise_poll_done() {
        let out = parse_precise_poll(
            r#"{"code":0,"data":{"extract_results":[{"state":"done","full_zip_url":"https://cdn/full.zip"}]}}"#,
        )
        .unwrap();
        assert_eq!(out.state, "done");
        assert_eq!(out.full_zip_url.as_deref(), Some("https://cdn/full.zip"));
    }

    #[test]
    fn test_extract_full_md_from_zip() {
        let bytes = zip_bytes(&[("full.md", b"# Hello"), ("x_content_list.json", b"{}")]);
        assert_eq!(extract_full_md_from_zip(&bytes).unwrap(), "# Hello");
    }

    #[test]
    fn test_extract_full_md_fallback() {
        let bytes = zip_bytes(&[("out.md", b"fallback md")]);
        assert_eq!(extract_full_md_from_zip(&bytes).unwrap(), "fallback md");
    }
}
