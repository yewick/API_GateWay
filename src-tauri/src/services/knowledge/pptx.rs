//! PPTX 解析：解包 `ppt/slides/slide*.xml` → 每页段落文本。

use std::io::{Cursor, Read};

use super::table::rows_to_markdown;

/// 提取 .pptx 文本（纯文本，页间空行分隔）。
pub fn extract_pptx(content: &[u8]) -> Result<String, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(content))
        .map_err(|e| format!("打开 pptx 失败: {e}"))?;

    let mut slides: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let name = archive
            .by_index(i)
            .map_err(|e| format!("读取 pptx 条目失败: {e}"))?
            .name()
            .to_string();
        if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
            slides.push(name);
        }
    }
    slides.sort_by_key(|n| slide_number(n));

    let mut out: Vec<String> = Vec::new();
    for name in slides {
        let mut xml = String::new();
        archive
            .by_name(&name)
            .map_err(|e| format!("读取 {name} 失败: {e}"))?
            .read_to_string(&mut xml)
            .map_err(|e| format!("读取 {name} 失败: {e}"))?;
        let text = slide_xml_to_text(&xml);
        if !text.trim().is_empty() {
            out.push(text);
        }
    }

    if out.is_empty() {
        return Err("无法从该 pptx 提取文本".to_string());
    }
    Ok(out.join("\n\n"))
}

/// 从 `slideN.xml` 文件名提取数字序号（`slide10.xml` → 10）。
fn slide_number(name: &str) -> u32 {
    name.rsplit("slide")
        .next()
        .and_then(|rest| rest.split('.').next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// `slideN.xml` → 段落文本（纯函数，供单测）。
/// 表格（`<a:tbl>`）渲染为 Markdown 表格；表格内的段落不再重复单独输出。
fn slide_xml_to_text(xml: &str) -> String {
    let doc = match roxmltree::Document::parse(xml) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };
    let mut paras: Vec<String> = Vec::new();
    for node in doc.descendants().filter(|n| n.is_element()) {
        match node.tag_name().name() {
            "tbl" => {
                if let Some(md) = table_to_markdown(node) {
                    paras.push(md);
                }
            }
            "p" if !is_in_table(node) => {
                let text: String = node
                    .descendants()
                    .filter(|n| n.is_element() && n.tag_name().name() == "t")
                    .filter_map(|n| n.text())
                    .collect();
                let text = text.trim().to_string();
                if !text.is_empty() {
                    paras.push(text);
                }
            }
            _ => {}
        }
    }
    paras.join("\n")
}

/// `<a:tbl>` → Markdown 表格（`a:tbl`→`a:tr`→`a:tc`→`a:p`/`a:t`）。
fn table_to_markdown(tbl: roxmltree::Node) -> Option<String> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for tr in tbl
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "tr")
    {
        let mut cells = Vec::new();
        for tc in tr
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "tc")
        {
            let cell: String = tc
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "t")
                .filter_map(|n| n.text())
                .collect();
            cells.push(cell.trim().to_string());
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
    }
    rows_to_markdown(&rows)
}

/// 判断节点是否位于某个表格单元格内（用于跳过表格内段落的重复输出）。
fn is_in_table(node: roxmltree::Node) -> bool {
    node.ancestors()
        .any(|a| a.is_element() && a.tag_name().name() == "tbl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slide_number() {
        assert_eq!(slide_number("ppt/slides/slide1.xml"), 1);
        assert_eq!(slide_number("ppt/slides/slide10.xml"), 10);
    }

    #[test]
    fn test_slide_xml_to_text() {
        let xml = r#"<?xml version="1.0"?><p:sld xmlns:p="urn:p" xmlns:a="urn:a">
            <p:sp><p:txBody><a:p><a:r><a:t>Hello</a:t></a:r><a:r><a:t> World</a:t></a:r></a:p>
            <a:p><a:r><a:t>第二行</a:t></a:r></a:p></p:txBody></p:sp></p:sld>"#;
        let text = slide_xml_to_text(xml);
        assert!(text.contains("Hello World"));
        assert!(text.contains("第二行"));
    }

    #[test]
    fn test_slide_xml_to_text_table() {
        // <a:tbl> 应渲染为 Markdown 表格，表格内段落不再重复输出
        let xml = r#"<?xml version="1.0"?><p:sld xmlns:p="urn:p" xmlns:a="urn:a">
            <p:graphicFrame><a:graphic><a:graphicData>
              <a:tbl>
                <a:tr>
                  <a:tc><a:txBody><a:p><a:r><a:t>A</a:t></a:r></a:p></a:txBody></a:tc>
                  <a:tc><a:txBody><a:p><a:r><a:t>B</a:t></a:r></a:p></a:txBody></a:tc>
                </a:tr>
                <a:tr>
                  <a:tc><a:txBody><a:p><a:r><a:t>1</a:t></a:r></a:p></a:txBody></a:tc>
                  <a:tc><a:txBody><a:p><a:r><a:t>2</a:t></a:r></a:p></a:txBody></a:tc>
                </a:tr>
              </a:tbl>
            </a:graphicData></a:graphic></p:graphicFrame>
            <p:sp><p:txBody><a:p><a:r><a:t>正文</a:t></a:r></a:p></p:txBody></p:sp>
        </p:sld>"#;
        let text = slide_xml_to_text(xml);
        assert!(text.contains("| A | B |"), "实际: {text}");
        assert!(text.contains("| --- | --- |"), "实际: {text}");
        assert!(text.contains("| 1 | 2 |"), "实际: {text}");
        assert!(text.contains("正文"), "实际: {text}");
        // 表格单元格文本不应作为独立段落重复出现
        assert!(!text.lines().any(|l| l.trim() == "A"), "实际: {text}");
    }
}
