//! 文档解析：将不同格式的文件提取为纯文本。
//!
//! 本次实现文本/代码类格式（txt / md / 代码文件）；pdf、docx、pptx、xlsx、html 等
//! 二进制/富格式解析需要额外依赖（pdf-extract / docx-rs / calamine 等），方案待定，
//! 本次对这类格式返回明确错误。

use super::pdf;

/// 解析后的纯文本文档
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    pub text: String,
    /// `text` / `markdown` / `code`（供 splitter 选择分块策略）
    pub file_type: String,
    /// 代码文件的语言名（如 `rust` / `python`），非代码为 `None`
    // 待 tree-sitter 符号提取接入后用于语言分派，当前尚未读取
    #[allow(dead_code)]
    pub language: Option<String>,
}

const MARKDOWN_EXTS: &[&str] = &["md", "markdown"];
/// 暂不支持、需后续实现的二进制/富格式扩展名（PDF 已单独走 [`super::pdf`] 后端）
const UNSUPPORTED_EXTS: &[&str] = &[
    "docx", "doc", "pptx", "ppt", "xlsx", "xls", "csv", "html", "htm",
];

/// 根据扩展名识别代码文件
pub fn is_code_file(ext: &str) -> bool {
    matches!(
        ext,
        "rs" | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "py"
            | "go"
            | "java"
            | "c"
            | "h"
            | "cpp"
            | "cc"
            | "hpp"
            | "cs"
            | "php"
            | "rb"
            | "swift"
            | "kt"
            | "kts"
            | "scala"
            | "sh"
            | "bash"
            | "sql"
            | "json"
            | "yaml"
            | "yml"
            | "toml"
    )
}

/// 代码文件扩展名 → 语言名
pub fn determine_language(ext: &str) -> Option<String> {
    let lang = match ext {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "cc" | "hpp" => "cpp",
        "cs" => "csharp",
        "php" => "php",
        "rb" => "ruby",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        "sh" | "bash" => "shell",
        "sql" => "sql",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        _ => return None,
    };
    Some(lang.to_string())
}

/// 取文件名小写扩展名（不含点）
pub fn extension(filename: &str) -> String {
    filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase()
}

/// 解析文档为纯文本。按扩展名分发：
/// - pdf → 经 [`super::pdf`] 可插拔后端提取 Markdown（Native 默认）
/// - txt / md / 代码文件 → 直接按 UTF-8 读取文本
/// - docx / pptx / xlsx / csv / html → 返回「未实现」错误（后续补充）
/// - 其他 → 尝试按 UTF-8 文本读取
pub fn parse_document(filename: &str, content: &[u8]) -> Result<ParsedDocument, String> {
    let ext = extension(filename);

    if ext == "pdf" {
        let text = pdf::extract_pdf_text(pdf::resolve_backend(), content)?;
        if text.trim().is_empty() {
            return Err("无法从该 PDF 提取文本（可能是扫描件/纯图片，OCR 尚未支持）".to_string());
        }
        return Ok(ParsedDocument {
            text,
            file_type: "markdown".to_string(),
            language: None,
        });
    }

    if UNSUPPORTED_EXTS.contains(&ext.as_str()) {
        return Err(format!(
            "不支持的文档格式 '.{}'：解析器尚未实现（pdf/docx/pptx/xlsx/csv/html 待后续补充）",
            ext
        ));
    }

    let text = String::from_utf8_lossy(content).to_string();

    let file_type = if MARKDOWN_EXTS.contains(&ext.as_str()) {
        "markdown"
    } else if is_code_file(&ext) {
        "code"
    } else {
        "text"
    };

    let language = if file_type == "code" {
        determine_language(&ext)
    } else {
        None
    };

    Ok(ParsedDocument {
        text,
        file_type: file_type.to_string(),
        language,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_lowercase() {
        assert_eq!(extension("README.MD"), "md");
        assert_eq!(extension("main.RS"), "rs");
        // 无扩展名时返回整个文件名（与 `rsplit('.').next()` 语义一致）
        assert_eq!(extension("noext"), "noext");
    }

    #[test]
    fn test_parse_markdown() {
        let doc = parse_document("guide.md", b"# Title\ncontent").unwrap();
        assert_eq!(doc.file_type, "markdown");
        assert_eq!(doc.language, None);
        assert_eq!(doc.text, "# Title\ncontent");
    }

    #[test]
    fn test_parse_rust_code() {
        let doc = parse_document("main.rs", b"fn main() {}").unwrap();
        assert_eq!(doc.file_type, "code");
        assert_eq!(doc.language.as_deref(), Some("rust"));
    }

    #[test]
    fn test_parse_python_code() {
        let doc = parse_document("app.py", b"def f():\n    pass").unwrap();
        assert_eq!(doc.file_type, "code");
        assert_eq!(doc.language.as_deref(), Some("python"));
    }

    #[test]
    fn test_parse_txt() {
        let doc = parse_document("notes.txt", b"plain text").unwrap();
        assert_eq!(doc.file_type, "text");
        assert_eq!(doc.language, None);
    }

    #[test]
    fn test_unsupported_format_errors() {
        assert!(parse_document("report.pdf", b"%PDF-1.4").is_err());
        assert!(parse_document("sheet.xlsx", b"PK\x03\x04").is_err());
    }

    #[test]
    fn test_is_code_file_and_language() {
        assert!(is_code_file("rs"));
        assert!(is_code_file("py"));
        assert!(!is_code_file("md"));
        assert_eq!(determine_language("go"), Some("go".to_string()));
        assert_eq!(determine_language("md"), None);
    }
}
