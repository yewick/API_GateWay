//! XLSX 解析：calamine → 每个工作表一张 Markdown 表格。

use std::io::Cursor;

use calamine::{open_workbook_auto_from_rs, Data, Reader};

use super::table::rows_to_markdown;

/// 提取 .xlsx 文本为 Markdown（每 sheet 一个 `##` 标题 + 表格）。
pub fn extract_xlsx(content: &[u8]) -> Result<String, String> {
    let mut workbook = open_workbook_auto_from_rs(Cursor::new(content))
        .map_err(|e| format!("打开 xlsx 失败: {e}"))?;

    let mut out: Vec<String> = Vec::new();
    for name in workbook.sheet_names() {
        let range = workbook
            .worksheet_range(&name)
            .map_err(|e| format!("读取工作表 {name} 失败: {e}"))?;
        let rows: Vec<Vec<String>> = range
            .rows()
            .map(|row| row.iter().map(cell_str).collect())
            .filter(|row: &Vec<String>| row.iter().any(|c| !c.is_empty()))
            .collect();
        if rows.is_empty() {
            continue;
        }
        let body = match rows_to_markdown(&rows) {
            Some(md) => md,
            None => rows
                .iter()
                .flat_map(|r| r.iter())
                .filter(|c| !c.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(" | "),
        };
        out.push(format!("## {name}\n\n{body}"));
    }

    if out.is_empty() {
        return Err("无法从该 xlsx 提取内容".to_string());
    }
    Ok(out.join("\n\n"))
}

fn cell_str(d: &Data) -> String {
    if matches!(d, Data::Empty) {
        String::new()
    } else {
        d.to_string()
    }
}
