//! 文档解析：按扩展名分发，把各格式提取为纯文本/Markdown。
//!
//! 支持：pdf（可插拔后端）、docx / xlsx / csv / pptx / html（各解析模块）、
//! txt / md / 代码文件（UTF-8 直读）。旧版二进制 doc / xls / ppt 暂不支持。

use tokio::sync::mpsc::UnboundedSender;

use super::{csv, docx, html, pdf, pptx, xlsx};

/// 解析后的纯文本文档
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    pub text: String,
    /// `text` / `markdown` / `code`（供 splitter 选择分块策略）
    pub file_type: String,
    /// 代码文件的语言名（如 `rust` / `python`），非代码为 `None`
    pub language: Option<String>,
}

const MARKDOWN_EXTS: &[&str] = &["md", "markdown"];
/// 旧版二进制（OLE2）格式，暂不支持，返回明确错误（需另存为新格式）
const UNSUPPORTED_EXTS: &[&str] = &["doc", "xls", "ppt"];

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
/// - docx / pptx / xlsx / csv / html → 各解析模块，产出 Markdown / 纯文本
/// - 其他 → 尝试按 UTF-8 文本读取
///
/// `app` 用于读取 store（`knowledge.pdf_backend` / `knowledge.mineru.*`）；`None` 时回退环境变量/默认值。
pub async fn parse_document(
    filename: &str,
    content: &[u8],
    progress: Option<UnboundedSender<pdf::ParseProgress>>,
    app: Option<&tauri::AppHandle>,
) -> Result<ParsedDocument, String> {
    let ext = extension(filename);

    if ext == "pdf" {
        let mineru_cfg = super::mineru::MinerUConfig::resolve(app);
        let backend = pdf::resolve_backend(app);
        let text =
            pdf::extract_pdf_text(backend, filename, content, progress, Some(mineru_cfg)).await?;
        if text.trim().is_empty() {
            return Err("无法从该 PDF 提取文本（可能是扫描件/纯图片，OCR 尚未支持）".to_string());
        }
        return Ok(ParsedDocument {
            text,
            file_type: "markdown".to_string(),
            language: None,
        });
    }

    // 办公/富文本格式 → 各解析模块，产出 Markdown / 纯文本
    match ext.as_str() {
        "docx" => return docx::extract_docx(content).map(|t| parsed(t, "markdown")),
        "xlsx" => return xlsx::extract_xlsx(content).map(|t| parsed(t, "markdown")),
        "csv" => return csv::extract_csv(content).map(|t| parsed(t, "markdown")),
        "pptx" => return pptx::extract_pptx(content).map(|t| parsed(t, "text")),
        "html" | "htm" => return html::extract_html(content).map(|t| parsed(t, "text")),
        _ => {}
    }

    if UNSUPPORTED_EXTS.contains(&ext.as_str()) {
        return Err(format!(
            "不支持的旧版格式 '.{}'：请另存为 .docx / .xlsx / .pptx 后再上传",
            ext
        ));
    }

    // 二进制文件（含 NUL 字节）无法按文本解析，直接报错，避免被 from_utf8_lossy
    // 解成垃圾文本后落成一条 failed 文档。放在文本回退之前，不影响 pdf/docx/xlsx
    // 等结构化格式（它们走上面的分支）。
    if content.contains(&0) {
        return Err(format!("'{filename}' 是二进制文件，无法解析为文本"));
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

/// 组装结构化解析结果。
fn parsed(text: String, file_type: &str) -> ParsedDocument {
    ParsedDocument {
        text,
        file_type: file_type.to_string(),
        language: None,
    }
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

    #[tokio::test]
    async fn test_parse_markdown() {
        let doc = parse_document("guide.md", b"# Title\ncontent", None, None)
            .await
            .unwrap();
        assert_eq!(doc.file_type, "markdown");
        assert_eq!(doc.language, None);
        assert_eq!(doc.text, "# Title\ncontent");
    }

    #[tokio::test]
    async fn test_parse_rust_code() {
        let doc = parse_document("main.rs", b"fn main() {}", None, None)
            .await
            .unwrap();
        assert_eq!(doc.file_type, "code");
        assert_eq!(doc.language.as_deref(), Some("rust"));
    }

    #[tokio::test]
    async fn test_parse_python_code() {
        let doc = parse_document("app.py", b"def f():\n    pass", None, None)
            .await
            .unwrap();
        assert_eq!(doc.file_type, "code");
        assert_eq!(doc.language.as_deref(), Some("python"));
    }

    #[tokio::test]
    async fn test_parse_txt() {
        let doc = parse_document("notes.txt", b"plain text", None, None)
            .await
            .unwrap();
        assert_eq!(doc.file_type, "text");
        assert_eq!(doc.language, None);
    }

    #[tokio::test]
    async fn test_unsupported_format_errors() {
        assert!(parse_document("report.pdf", b"%PDF-1.4", None, None).await.is_err());
        // 旧版 OLE2 二进制格式（doc/xls/ppt）→ 明确报错
        assert!(parse_document("sheet.xls", b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1", None, None)
            .await
            .is_err());
        assert!(parse_document("old.doc", b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1", None, None)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_binary_file_errors() {
        // PNG 魔数 + 数据段 NUL 字节 → 判为二进制并报错，而不是解成垃圾文本
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00];
        assert!(parse_document("icon.png", &png, None, None).await.is_err());
        // 纯文本不受影响
        assert!(parse_document("notes.txt", b"plain text", None, None)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_parse_csv() {
        let doc = parse_document("data.csv", b"a,b\n1,2\n", None, None)
            .await
            .unwrap();
        assert_eq!(doc.file_type, "markdown");
        assert!(doc.text.contains("| a | b |"));
        assert!(doc.text.contains("| 1 | 2 |"));
    }

    #[tokio::test]
    async fn test_parse_html() {
        let doc = parse_document(
            "page.html",
            b"<html><body><h1>Hi</h1><p>there</p></body></html>",
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(doc.file_type, "text");
        assert!(doc.text.contains("Hi"));
        assert!(doc.text.contains("there"));
    }

    #[tokio::test]
    async fn test_parse_docx_end_to_end() {
        let document_xml = r#"<?xml version="1.0"?><w:document xmlns:w="urn:x"><w:body><w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>个人信息</w:t></w:r></w:p><w:p><w:r><w:t>熟练掌握 Spring Boot</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>技术</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Spring</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>框架</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Boot</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#;
        let bytes = zip_with(&[("word/document.xml", document_xml.as_bytes())]);
        let doc = parse_document("cv.docx", &bytes, None, None).await.unwrap();
        assert_eq!(doc.file_type, "markdown");
        assert!(doc.text.contains("# 个人信息"));
        assert!(doc.text.contains("熟练掌握 Spring Boot"));
        assert!(doc.text.contains("| 技术 | Spring |"));
        assert!(doc.text.contains("| 框架 | Boot |"));
    }

    #[tokio::test]
    async fn test_parse_xlsx_end_to_end() {
        let bytes = zip_with(&[
            ("[Content_Types].xml", XLSX_CT.as_bytes()),
            ("_rels/.rels", XLSX_RELS.as_bytes()),
            ("xl/workbook.xml", XLSX_WB.as_bytes()),
            ("xl/_rels/workbook.xml.rels", XLSX_WB_RELS.as_bytes()),
            ("xl/worksheets/sheet1.xml", XLSX_SHEET1.as_bytes()),
        ]);
        let doc = parse_document("data.xlsx", &bytes, None, None).await.unwrap();
        assert_eq!(doc.file_type, "markdown");
        assert!(doc.text.contains("## Sheet1"));
        assert!(doc.text.contains("| 技术 | Spring |"));
        assert!(doc.text.contains("| 框架 | Boot |"));
    }

    fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
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

    const XLSX_CT: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
    const XLSX_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    const XLSX_WB: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    const XLSX_WB_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
    const XLSX_SHEET1: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>技术</t></is></c><c r="B1" t="inlineStr"><is><t>Spring</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>框架</t></is></c><c r="B2" t="inlineStr"><is><t>Boot</t></is></c></row></sheetData></worksheet>"#;

    #[test]
    fn test_is_code_file_and_language() {
        assert!(is_code_file("rs"));
        assert!(is_code_file("py"));
        assert!(!is_code_file("md"));
        assert_eq!(determine_language("go"), Some("go".to_string()));
        assert_eq!(determine_language("md"), None);
    }
}
