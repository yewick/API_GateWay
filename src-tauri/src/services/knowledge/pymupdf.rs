//! PyMuPDF 后端：Python 子进程提取 PDF → JSON blocks → 结构分析 → Markdown。
//!
//! 管道：PDF 字节 → 临时 `.pdf` + 内嵌 `pymupdf_extract.py` → `python` 子进程
//! → stdout JSON（见 [`PdfBlock`]）→ [`blocks_to_markdown`] 布局/字体/坐标分析 → Markdown。

use std::process::Command;

use serde::Deserialize;

use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;

use super::pdf::{ParseProgress, PdfExtractor};

/// Python 脚本（编译期内嵌，运行时写临时文件）
const PY_SCRIPT: &str = include_str!("pymupdf_extract.py");

/// PyMuPDF 后端
pub struct PyMuPdfExtractor;

#[async_trait]
impl PdfExtractor for PyMuPdfExtractor {
    async fn extract(
        &self,
        _filename: &str,
        content: &[u8],
        _progress: Option<UnboundedSender<ParseProgress>>,
    ) -> Result<String, String> {
        let json = run_python(content)?;
        let blocks: Vec<PdfBlock> =
            serde_json::from_str(&json).map_err(|e| format!("PyMuPDF 输出解析失败: {e}"))?;
        Ok(blocks_to_markdown(&blocks))
    }
}

// ---------------------------------------------------------------------------
// JSON 契约（Python stdout ↔ Rust）
// ---------------------------------------------------------------------------

#[allow(dead_code)] // 部分字段（font 等）为后续结构分析预留
#[derive(Debug, Deserialize)]
struct PdfBlock {
    #[serde(default)]
    page: u32,
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    bbox: [f32; 4],
    #[serde(default)]
    lines: Vec<PdfLine>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct PdfLine {
    #[serde(default)]
    bbox: [f32; 4],
    #[serde(default)]
    spans: Vec<PdfSpan>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
struct PdfSpan {
    text: String,
    #[serde(default)]
    size: f32,
    #[serde(default)]
    font: String,
    #[serde(default)]
    flags: u32,
    #[serde(default)]
    bbox: [f32; 4],
}

// ---------------------------------------------------------------------------
// 子进程
// ---------------------------------------------------------------------------

/// 找可用的 Python 解释器：`YEAPI_PYTHON` → `python3` → `python`。
fn find_python() -> Option<String> {
    if let Ok(p) = std::env::var("YEAPI_PYTHON") {
        if !p.trim().is_empty() {
            return Some(p);
        }
    }
    for candidate in ["python3", "python"] {
        if Command::new(candidate).arg("--version").output().is_ok() {
            return Some(candidate.to_string());
        }
    }
    None
}

/// 临时文件清理守卫（Drop 时删除，保证异常路径也不残留）。
struct TempGuard {
    files: Vec<std::path::PathBuf>,
}

impl Drop for TempGuard {
    fn drop(&mut self) {
        for f in &self.files {
            let _ = std::fs::remove_file(f);
        }
    }
}

/// 起 Python 子进程提取 PDF，返回 stdout（JSON 字符串）。
fn run_python(pdf_bytes: &[u8]) -> Result<String, String> {
    let python = find_python().ok_or_else(|| {
        "未找到 Python 解释器（尝试 YEAPI_PYTHON / python3 / python）。\
         使用 PyMuPDF 后端需 `pip install pymupdf`"
            .to_string()
    })?;

    let tmp = std::env::temp_dir();
    let tag = uuid::Uuid::new_v4();
    let script_path = tmp.join(format!("yeapi_pymupdf_{tag}.py"));
    let pdf_path = tmp.join(format!("yeapi_pymupdf_{tag}.pdf"));
    let _guard = TempGuard {
        files: vec![script_path.clone(), pdf_path.clone()],
    };

    std::fs::write(&script_path, PY_SCRIPT).map_err(|e| format!("写入临时脚本失败: {e}"))?;
    std::fs::write(&pdf_path, pdf_bytes).map_err(|e| format!("写入临时 PDF 失败: {e}"))?;

    // 无 shell、参数直接传递，避免注入
    let output = Command::new(&python)
        .arg(&script_path)
        .arg(&pdf_path)
        .output()
        .map_err(|e| format!("启动 Python 失败: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("PyMuPDF 提取失败: {}", stderr.trim()));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("PyMuPDF 输出非 UTF-8: {e}"))
}

// ---------------------------------------------------------------------------
// 结构分析：JSON blocks → Markdown
// ---------------------------------------------------------------------------

/// 正文字号基线：所有文本 span 字号的中位数。
fn body_font_size(blocks: &[PdfBlock]) -> f32 {
    let mut sizes: Vec<f32> = blocks
        .iter()
        .flat_map(|b| b.lines.iter())
        .flat_map(|l| l.spans.iter())
        .map(|s| s.size)
        .filter(|s| *s > 0.0)
        .collect();
    if sizes.is_empty() {
        return 0.0;
    }
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    sizes[sizes.len() / 2]
}

/// 判断 block 是否为标题：字号明显大于正文，或加粗的短单行。
fn heading_level(block: &PdfBlock, body: f32) -> Option<u32> {
    if block.lines.is_empty() {
        return None;
    }
    let spans: Vec<&PdfSpan> = block.lines.iter().flat_map(|l| &l.spans).collect();
    let char_count: usize = spans.iter().map(|s| s.text.chars().count()).sum();
    if char_count > 80 {
        return None; // 过长，是段落而非标题
    }
    let max_size = spans.iter().map(|s| s.size).fold(0.0_f32, f32::max);
    let ratio = if body > 0.0 { max_size / body } else { 1.0 };
    if ratio >= 1.5 {
        return Some(1);
    }
    if ratio >= 1.3 {
        return Some(2);
    }
    // 加粗的短单行（简历常见小节标题）→ 三级标题
    let all_bold = spans.iter().all(|s| s.flags & 16 != 0);
    if all_bold && block.lines.len() == 1 && char_count <= 60 {
        return Some(3);
    }
    None
}

/// 同一表格行的 y 容差（点）。
const ROW_TOLERANCE: f32 = 5.0;
/// 跨行 x0 对齐容差（点）。
const TABLE_TOLERANCE: f32 = 8.0;

/// 尝试把 block 重建为 Markdown 表格；对齐失败返回 None（按普通段落处理）。
///
/// PyMuPDF 的 `get_text("dict")` 把表格拆成「每 cell 一条 line」（同行 cell 的 y 相同），
/// 因此先按 y 聚类成「行」，行内按 x 排序为 cell。
fn try_render_table(block: &PdfBlock) -> Option<String> {
    if block.lines.len() < 4 {
        return None; // 至少 2 行 × 2 列
    }

    // 按 y0 聚类为行
    let mut sorted: Vec<&PdfLine> = block.lines.iter().collect();
    sorted.sort_by(|a, b| {
        a.bbox[1]
            .partial_cmp(&b.bbox[1])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut rows: Vec<Vec<&PdfLine>> = Vec::new();
    for line in sorted {
        match rows.last_mut() {
            Some(last) if (line.bbox[1] - last[0].bbox[1]).abs() <= ROW_TOLERANCE => {
                last.push(line);
            }
            _ => rows.push(vec![line]),
        }
    }
    if rows.len() < 2 {
        return None;
    }

    // 行内按 x0 排序为 cell；列数需一致且 >= 2
    for row in &mut rows {
        row.sort_by(|a, b| {
            a.bbox[0]
                .partial_cmp(&b.bbox[0])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    let ncols = rows[0].len();
    if ncols < 2 {
        return None;
    }
    if rows.iter().any(|r| r.len() != ncols) {
        return None;
    }

    // 列 x0 对齐验证：以第一行各 cell 的 x0 为基准
    let col_x: Vec<f32> = rows[0].iter().map(|l| l.bbox[0]).collect();
    for row in &rows[1..] {
        for (i, line) in row.iter().enumerate() {
            if (line.bbox[0] - col_x[i]).abs() > TABLE_TOLERANCE {
                return None;
            }
        }
    }

    let mut md = String::new();
    for (ri, row) in rows.iter().enumerate() {
        md.push('|');
        for line in row {
            let cell = line_text(line).replace('|', "\\|").replace('\n', " ");
            md.push(' ');
            md.push_str(&cell);
            md.push_str(" |");
        }
        md.push('\n');
        if ri == 0 {
            md.push('|');
            for _ in 0..ncols {
                md.push_str(" --- |");
            }
            md.push('\n');
        }
    }
    Some(md)
}

fn line_text(line: &PdfLine) -> String {
    line.spans
        .iter()
        .map(|s| s.text.as_str())
        .collect::<String>()
        .trim()
        .to_string()
}

/// block 内全部文本（行间以空格连接，行内 spans 直接拼接）。
fn block_text(block: &PdfBlock) -> String {
    block
        .lines
        .iter()
        .map(line_text)
        .collect::<Vec<_>>()
        .join(" ")
}

/// JSON blocks → Markdown（表格 → 标题 → 正文；图片块跳过，仅保留文本）。
fn blocks_to_markdown(blocks: &[PdfBlock]) -> String {
    // 阅读顺序：按 (page, y0, x0) 排序
    let mut ordered: Vec<&PdfBlock> = blocks.iter().collect();
    ordered.sort_by(|a, b| {
        (a.page, a.bbox[1], a.bbox[0])
            .partial_cmp(&(b.page, b.bbox[1], b.bbox[0]))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let body = body_font_size(blocks);

    let mut out = String::new();
    for block in ordered {
        if block.block_type == "image" || block.lines.is_empty() {
            continue; // 仅文本；图片说明本就以 text block 形式保留
        }
        if let Some(table) = try_render_table(block) {
            out.push_str(&table);
            out.push('\n');
        } else if let Some(level) = heading_level(block, body) {
            out.push_str(&"#".repeat(level as usize));
            out.push(' ');
            out.push_str(&block_text(block));
            out.push_str("\n\n");
        } else {
            out.push_str(&block_text(block));
            out.push_str("\n\n");
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str, size: f32, x0: f32, flags: u32) -> PdfSpan {
        PdfSpan {
            text: text.to_string(),
            size,
            font: String::new(),
            flags,
            bbox: [x0, 0.0, x0 + 10.0, 10.0],
        }
    }

    fn line_at(spans: Vec<PdfSpan>, x0: f32, y0: f32) -> PdfLine {
        PdfLine {
            bbox: [x0, y0, x0 + 100.0, y0 + 10.0],
            spans,
        }
    }

    fn block(lines: Vec<PdfLine>) -> PdfBlock {
        PdfBlock {
            page: 1,
            block_type: "text".to_string(),
            bbox: [0.0, 0.0, 300.0, 300.0],
            lines,
        }
    }

    #[test]
    fn test_body_font_size_median() {
        let blocks = vec![block(vec![line_at(
            vec![
                span("a", 10.0, 0.0, 0),
                span("b", 12.0, 0.0, 0),
                span("c", 20.0, 0.0, 0),
            ],
            0.0,
            0.0,
        )])];
        assert!((body_font_size(&blocks) - 12.0).abs() < 0.001);
    }

    #[test]
    fn test_heading_and_paragraph() {
        let mut body_lines = Vec::new();
        for i in 0..3 {
            body_lines.push(line_at(
                vec![span("正文内容若干", 10.0, 0.0, 0)],
                0.0,
                100.0 + i as f32 * 14.0,
            ));
        }
        let blocks = vec![
            block(vec![line_at(vec![span("个人信息", 15.0, 0.0, 0)], 0.0, 0.0)]),
            block(body_lines),
        ];
        let md = blocks_to_markdown(&blocks);
        assert!(md.contains("# 个人信息"));
        assert!(md.contains("正文内容若干"));
    }

    #[test]
    fn test_bold_heading() {
        let mut body_lines = Vec::new();
        for i in 0..4 {
            body_lines.push(line_at(
                vec![span("正文", 10.0, 0.0, 0)],
                0.0,
                100.0 + i as f32 * 14.0,
            ));
        }
        let blocks = vec![
            block(vec![line_at(vec![span("专业技能", 10.0, 0.0, 16)], 0.0, 0.0)]),
            block(body_lines),
        ];
        let md = blocks_to_markdown(&blocks);
        assert!(md.contains("### 专业技能"));
    }

    #[test]
    fn test_table_detection() {
        // 2 行 × 2 列：每 cell 一条 line，同行 y 相同
        let blocks = vec![block(vec![
            line_at(vec![span("技术", 10.0, 0.0, 0)], 0.0, 100.0),
            line_at(vec![span("Spring", 10.0, 80.0, 0)], 80.0, 100.0),
            line_at(vec![span("框架", 10.0, 0.0, 0)], 0.0, 114.0),
            line_at(vec![span("Boot", 10.0, 80.0, 0)], 80.0, 114.0),
        ])];
        let md = blocks_to_markdown(&blocks);
        assert!(md.contains("| 技术 | Spring |"));
        assert!(md.contains("| --- | --- |"));
        assert!(md.contains("| 框架 | Boot |"));
    }

    #[test]
    fn test_table_misaligned_falls_back() {
        // 第二行第二列 x0 偏移超过容差 → 不当作表格
        let blocks = vec![block(vec![
            line_at(vec![span("技术", 10.0, 0.0, 0)], 0.0, 100.0),
            line_at(vec![span("Spring", 10.0, 80.0, 0)], 80.0, 100.0),
            line_at(vec![span("框架", 10.0, 0.0, 0)], 0.0, 114.0),
            line_at(vec![span("Boot", 10.0, 95.0, 0)], 95.0, 114.0),
        ])];
        let md = blocks_to_markdown(&blocks);
        assert!(!md.contains("| --- |"));
    }
}
