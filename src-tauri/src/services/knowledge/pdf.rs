//! PDF 文字提取后端（可插拔）。
//!
//! 三种后端：
//! - [`PdfBackend::Native`]：纯 Rust 的 `unpdf`（默认），零外部依赖，原生 CMap/中文解码。
//! - [`PdfBackend::PyMuPDF`]：Python 子进程调用 pymupdf，保留文档结构（标题/表格/正文）。
//! - [`PdfBackend::MinerU`]：自托管 MinerU HTTP 服务（待实现，接口已预留）。
//!
//! 分发层统一走 [`extract_pdf_text`]，各后端实现 [`PdfExtractor`] trait；
//! 输出统一经 [`normalize`] 做 NFKC 归一化（修康熙部首/全角/零宽字符）。

use std::str::FromStr;

use unicode_normalization::UnicodeNormalization;

use super::mineru::MinerUExtractor;
use super::pymupdf::PyMuPdfExtractor;

/// PDF 解析后端（选择/配置层）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfBackend {
    /// 纯 Rust `unpdf`（默认，零依赖）
    Native,
    /// Python 子进程调用 pymupdf（保留结构）
    PyMuPDF,
    /// 自托管 MinerU HTTP 服务（待实现）
    MinerU,
}

impl Default for PdfBackend {
    fn default() -> Self {
        Self::Native
    }
}

impl FromStr for PdfBackend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "native" | "unpdf" => Ok(Self::Native),
            "pymupdf" | "mupdf" => Ok(Self::PyMuPDF),
            "mineru" => Ok(Self::MinerU),
            other => Err(format!(
                "未知的 PDF 后端: {other}（可选 native / pymupdf / mineru）"
            )),
        }
    }
}

/// PDF 提取器接口：未来新增后端（如 MinerU）只需实现此 trait，分发层零改动。
pub trait PdfExtractor {
    /// PDF 字节 → Markdown 文本
    fn extract(&self, content: &[u8]) -> Result<String, String>;
}

/// Native 后端：`unpdf` 纯 Rust 解析 → Markdown
pub struct NativeExtractor;

impl PdfExtractor for NativeExtractor {
    fn extract(&self, content: &[u8]) -> Result<String, String> {
        let doc = unpdf::parse_bytes(content).map_err(|e| format!("PDF 解析失败: {e}"))?;
        let opts = unpdf::render::RenderOptions::default();
        unpdf::render::to_markdown(&doc, &opts).map_err(|e| format!("PDF 文本提取失败: {e}"))
    }
}

/// 解析后端选择：默认 `native`，可用环境变量 `YEAPI_PDF_BACKEND` 覆盖（native/pymupdf/mineru）。
pub fn resolve_backend() -> PdfBackend {
    std::env::var("YEAPI_PDF_BACKEND")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| PdfBackend::from_str(&s).unwrap_or_default())
        .unwrap_or_default()
}

/// PDF → Markdown 文本（统一入口：按后端分发，再统一做 NFKC 归一化）
pub fn extract_pdf_text(backend: PdfBackend, content: &[u8]) -> Result<String, String> {
    let text = match backend {
        PdfBackend::Native => NativeExtractor.extract(content),
        PdfBackend::PyMuPDF => PyMuPdfExtractor.extract(content),
        PdfBackend::MinerU => MinerUExtractor.extract(content),
    }?;
    Ok(normalize(&text))
}

/// 共享后处理：NFKC 归一化（康熙部首→标准汉字、全角→半角），并剔除零宽字符。
fn normalize(s: &str) -> String {
    s.nfkc()
        .filter(|c| !matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_from_str() {
        assert_eq!(PdfBackend::from_str("native").unwrap(), PdfBackend::Native);
        assert_eq!(
            PdfBackend::from_str("pymupdf").unwrap(),
            PdfBackend::PyMuPDF
        );
        assert_eq!(PdfBackend::from_str("MinerU").unwrap(), PdfBackend::MinerU);
        assert!(PdfBackend::from_str("nope").is_err());
    }

    #[test]
    fn test_default_native() {
        assert_eq!(PdfBackend::default(), PdfBackend::Native);
    }

    #[test]
    fn test_unimplemented_backends_error() {
        assert!(extract_pdf_text(PdfBackend::MinerU, b"whatever").is_err());
    }

    #[test]
    fn test_native_invalid_pdf_errors() {
        assert!(extract_pdf_text(PdfBackend::Native, b"not a real pdf").is_err());
    }

    #[test]
    fn test_normalize_kangxi_radicals() {
        // ⽤ (U+2F64) → 用 (U+7528)，⾃ (U+2F83) → 自 (U+81EA)
        assert_eq!(normalize("使⽤⾃动"), "使用自动");
    }

    #[test]
    fn test_normalize_fullwidth() {
        assert_eq!(normalize("ＡＢＣ"), "ABC");
    }

    #[test]
    fn test_normalize_strips_zero_width() {
        assert_eq!(normalize("a\u{200B}b"), "ab");
    }
}
