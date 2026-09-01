//! CJK Bigram 分词与 FTS5 查询构建。
//!
//! FTS5 默认 `unicode61` 分词器对「连续 CJK 字符串」整体当成一个 token（例如
//! `如何处理并发安全` 是一个 8 字 token），导致中文子串查询几乎无法命中。本模块把
//! 连续 CJK 段切成二元组（bigram），中文查询与中文入库共用同一套分词，从而让
//! `"并发"* OR "安全"*` 这类前缀词能命中索引。
//!
//! - [`tokenize_query`]：对查询串分词（英文按词、中文按 bigram），供 [`build_fts_query`] 使用。
//! - [`tokenize_content`]：对入库正文分词并空格拼接，供 FTS 索引写入使用（见 `repository.rs`）。

use std::collections::HashSet;

/// 判断是否为 CJK 表意文字（CJK 统一表意文字 / 扩展 A / 兼容表意文字）。
/// 供 FTS 分词与 token 估算（`splitter::token_count`）共用，保证二者对「CJK 字符」的判定一致。
pub(crate) fn is_cjk_char(ch: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&ch)
        || ('\u{3400}'..='\u{4dbf}').contains(&ch)
        || ('\u{f900}'..='\u{faff}').contains(&ch)
}

/// 对查询串分词：
/// - 连续 CJK 段 → 滑动窗口生成 bigram（单字则直接作为一个 token）；
/// - 连续字母/数字段 → 保留 2 字符以上的整词；
/// - 空白 / 标点 / 其他符号 → 跳过。
///
/// 结果去重且保序。
pub fn tokenize_query(query: &str) -> Vec<String> {
    let chars: Vec<char> = query.chars().collect();
    let mut tokens: Vec<String> = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        if is_cjk_char(ch) {
            // 收集连续 CJK 段
            let start = i;
            while i < chars.len() && is_cjk_char(chars[i]) {
                i += 1;
            }
            let run = &chars[start..i];
            if run.len() == 1 {
                tokens.push(run[0].to_string());
            } else {
                for w in run.windows(2) {
                    tokens.push(format!("{}{}", w[0], w[1]));
                }
            }
        } else if ch.is_alphanumeric() {
            // 收集连续字母/数字单词（避免把后续 CJK 吞进英文词）
            let mut word = String::new();
            while i < chars.len() && chars[i].is_alphanumeric() && !is_cjk_char(chars[i]) {
                word.push(chars[i]);
                i += 1;
            }
            if word.chars().count() >= 2 {
                tokens.push(word);
            }
        } else {
            // 分隔符 / 其他符号
            i += 1;
        }
    }

    // 去重且保序
    let mut seen = HashSet::new();
    tokens.into_iter().filter(|t| seen.insert(t.clone())).collect()
}

/// 对入库正文分词，并以空格拼接，供 FTS 索引写入。与查询侧 [`tokenize_query`] 一致，
/// 保证查询 bigram 与索引 bigram 对齐。
pub fn tokenize_content(content: &str) -> String {
    tokenize_query(content).join(" ")
}

/// 构建 FTS5 MATCH 查询串：token 用引号包裹 + 前缀 `*`，以 ` OR ` 连接以提升召回。
/// 引号内先剥离 `"` 防止注入；无 token 时回退原查询串。
pub fn build_fts_query(query: &str) -> String {
    let tokens = tokenize_query(query);
    if tokens.is_empty() {
        return query.to_string();
    }
    tokens
        .iter()
        .map(|t| format!("\"{}\"*", t.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_cjk_bigram() {
        // "并发安全" → 并发/发安/安全
        assert_eq!(
            tokenize_query("并发安全"),
            vec!["并发".to_string(), "发安".to_string(), "安全".to_string()]
        );
    }

    #[test]
    fn test_tokenize_single_cjk_char() {
        assert_eq!(tokenize_query("安"), vec!["安".to_string()]);
    }

    #[test]
    fn test_tokenize_english_words() {
        assert_eq!(
            tokenize_query("hello world"),
            vec!["hello".to_string(), "world".to_string()]
        );
    }

    #[test]
    fn test_tokenize_short_word_dropped() {
        // 单字符英文词不保留
        assert_eq!(tokenize_query("a b"), Vec::<String>::new());
    }

    #[test]
    fn test_tokenize_mixed() {
        // "Rust 错误处理" → Rust + 错误/误处/处理（英文保留原大小写，FTS 层做大小写折叠）
        let tokens = tokenize_query("Rust 错误处理");
        assert!(tokens.contains(&"Rust".to_string()));
        assert!(tokens.contains(&"错误".to_string()));
        assert!(tokens.contains(&"处理".to_string()));
    }

    #[test]
    fn test_tokenize_dedup_preserves_order() {
        // "安全 安全" 去重后只剩一个
        assert_eq!(tokenize_query("安全 安全"), vec!["安全".to_string()]);
    }

    #[test]
    fn test_build_fts_query_or_join() {
        let q = build_fts_query("并发安全");
        assert_eq!(q, "\"并发\"* OR \"发安\"* OR \"安全\"*");
    }

    #[test]
    fn test_build_fts_query_quotes_split_as_separator() {
        // 引号是分隔符：`he"llo` 被切分到两个 token，各自引号包裹，不破坏 FTS 查询语法
        let q = build_fts_query("he\"llo");
        assert_eq!(q, "\"he\"* OR \"llo\"*");
    }

    #[test]
    fn test_build_fts_query_empty_falls_back() {
        assert_eq!(build_fts_query("!!!"), "!!!".to_string());
    }
}
