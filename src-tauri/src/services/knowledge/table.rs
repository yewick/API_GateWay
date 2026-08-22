//! 共享的 Markdown 表格渲染（docx / xlsx / csv 复用）。

/// 把二维字符串渲染为 Markdown 表格（首行作表头）。少于 2 行或无列返回 `None`。
pub(crate) fn rows_to_markdown(rows: &[Vec<String>]) -> Option<String> {
    if rows.len() < 2 {
        return None;
    }
    let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if ncols == 0 {
        return None;
    }

    let mut md = String::new();
    for (ri, row) in rows.iter().enumerate() {
        md.push('|');
        for ci in 0..ncols {
            let cell = row.get(ci).map(|s| escape_cell(s)).unwrap_or_default();
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

fn escape_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_table() {
        let rows = vec![
            vec!["A".to_string(), "B".to_string()],
            vec!["1".to_string(), "2".to_string()],
        ];
        let md = rows_to_markdown(&rows).unwrap();
        assert!(md.contains("| A | B |"));
        assert!(md.contains("| --- | --- |"));
        assert!(md.contains("| 1 | 2 |"));
    }

    #[test]
    fn test_escapes_pipe() {
        let rows = vec![vec!["a|b".to_string()], vec!["c".to_string()]];
        let md = rows_to_markdown(&rows).unwrap();
        assert!(md.contains("a\\|b"));
    }

    #[test]
    fn test_single_row_none() {
        let rows = vec![vec!["only".to_string()]];
        assert!(rows_to_markdown(&rows).is_none());
    }

    #[test]
    fn test_ragged_rows_padded() {
        let rows = vec![
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
            vec!["1".to_string()],
        ];
        let md = rows_to_markdown(&rows).unwrap();
        assert!(md.contains("| 1 |  |  |"));
    }
}
