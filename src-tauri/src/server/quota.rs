//! 配额预检：用「输入估算 + 输出预算」替代把 `max_tokens` 当预计消耗的做法。
//!
//! 配额是累计实际消耗的预算；`max_tokens` 只是单次输出上限。预检只判断
//! 「剩余额度能否覆盖本次输入 + 最小输出」，并把上游的 `max_tokens` 收敛到
//! 剩余预算，既保留硬性限额，又不会误拒正常的长上限请求。

use crate::services::knowledge::splitter;
use serde_json::Value;

/// 允许一次请求的最小输出预算；低于它认为配额不足以完成请求。
pub const MIN_OUTPUT_TOKENS: i64 = 64;
/// 输入估算的安全系数（+10%），抵消不同分词器之间的误差。
const SAFETY_NUM: i64 = 11;
const SAFETY_DEN: i64 = 10;
/// 每张图片的固定估算，避免把 base64 内容按字符数放大。
const IMAGE_TOKENS: i64 = 1000;
/// 消息结构、角色标记等固定开销。
const MESSAGE_OVERHEAD: i64 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaBudget {
    pub estimated_input: i64,
    pub output_budget: i64,
}

/// 限额 Key 的剩余额度；`None` 表示不限量。
pub fn remaining(quota_limit: i64, quota_used: i64) -> Option<i64> {
    (quota_limit > 0).then(|| quota_limit.saturating_sub(quota_used))
}

/// 估算请求输入 tokens（system + messages + tools），跳过图片 base64。
pub fn estimate_input_tokens(body: &Value) -> i64 {
    let mut text = String::new();
    let mut images = 0i64;
    collect_text(body, &mut text, &mut images);

    let base = splitter::token_count(&text) as i64;
    base.saturating_add(images.saturating_mul(IMAGE_TOKENS))
        .saturating_mul(SAFETY_NUM)
        .saturating_div(SAFETY_DEN)
        .saturating_add(MESSAGE_OVERHEAD)
}

fn collect_text(value: &Value, out: &mut String, images: &mut i64) {
    match value {
        Value::String(text) => {
            out.push_str(text);
            out.push('\n');
        }
        Value::Array(items) => {
            for item in items {
                collect_text(item, out, images);
            }
        }
        Value::Object(map) => {
            // 图片对象整体按固定值计，跳过 base64 正文。
            if map
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "image" || kind == "input_image")
            {
                *images = images.saturating_add(1);
                return;
            }
            for (key, item) in map {
                match key.as_str() {
                    "image_url" => {
                        *images = images.saturating_add(1);
                    }
                    // 只有长字符串才视为 base64；工具 schema 里名为 data 的字段照常估算。
                    "data" | "file_data" if item.as_str().is_some_and(|text| text.len() > 4096) => {
                        *images = images.saturating_add(1);
                    }
                    // Anthropic 图片 source：{ type: "base64" | "url", ... }。
                    "source"
                        if item
                            .get("type")
                            .and_then(Value::as_str)
                            .is_some_and(|kind| kind == "base64" || kind == "url") =>
                    {
                        *images = images.saturating_add(1);
                    }
                    _ => collect_text(item, out, images),
                }
            }
        }
        _ => {}
    }
}

/// 判断本次请求是否允许，并给出可用的输出预算。
///
/// - 不限量：始终允许，`output_budget` 等于请求的 `max_tokens`（未指定时为 0）。
/// - 限额：剩余额度不足以覆盖「估算输入 + 最小输出」时返回错误；
///   否则输出预算为 `剩余 - 估算输入`，并受客户端 `max_tokens` 上限约束。
pub fn decide_quota(
    remaining: Option<i64>,
    estimated_input: i64,
    requested_max_tokens: i64,
) -> Result<QuotaBudget, String> {
    let requested = requested_max_tokens.max(0);
    let Some(remaining) = remaining else {
        return Ok(QuotaBudget {
            estimated_input,
            output_budget: requested,
        });
    };

    if remaining <= 0 {
        return Err("Quota exceeded (the API key's token quota is used up)".to_string());
    }
    if remaining <= estimated_input.saturating_add(MIN_OUTPUT_TOKENS) {
        return Err(format!(
            "Quota insufficient (remaining: {}, estimated_input: {}, minimum output: {}). Increase the key quota or use an unlimited key.",
            remaining, estimated_input, MIN_OUTPUT_TOKENS
        ));
    }

    let available = remaining - estimated_input;
    let output_budget = if requested <= 0 {
        available
    } else {
        requested.min(available)
    };
    Ok(QuotaBudget {
        estimated_input,
        output_budget,
    })
}

/// 需要写入上游的 `max_tokens`；`None` 表示保持客户端原值。
pub fn clamp_max_tokens(quota: Option<&QuotaBudget>, requested: i64) -> Option<i64> {
    let budget = quota?;
    if requested <= 0 {
        return None;
    }
    Some(budget.output_budget.max(1).min(requested))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn estimates_text_and_tools() {
        let body = json!({
            "system": "You are a helpful assistant.",
            "messages": [
                {"role": "user", "content": "hello world"},
                {"role": "assistant", "content": "hi"}
            ],
            "tools": [{"type": "function", "function": {"name": "read_file", "description": "read a file"}}]
        });
        let estimate = estimate_input_tokens(&body);
        assert!(estimate > 0);
        assert!(estimate < 200);
    }

    #[test]
    fn skips_base64_images_and_counts_fixed_allowance() {
        let big = "A".repeat(200_000);
        let body = json!({
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": "what is this"},
                {"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{big}")}}
            ]}]
        });
        let estimate = estimate_input_tokens(&body);
        assert!(estimate < 10_000, "base64 不应按字符数放大: {estimate}");
        assert!(
            estimate >= IMAGE_TOKENS,
            "至少包含一次图片固定估算: {estimate}"
        );
    }

    #[test]
    fn short_data_fields_are_not_mistaken_for_images() {
        let body = json!({
            "tools": [{
                "type": "function",
                "function": {
                    "name": "save",
                    "parameters": { "properties": { "data": { "type": "string" } } }
                }
            }]
        });
        let estimate = estimate_input_tokens(&body);
        assert!(
            estimate < 500,
            "工具 schema 的 data 字段不应按图片计: {estimate}"
        );
    }

    #[test]
    fn unlimited_always_allows() {
        let budget = decide_quota(None, 12_000, 32_000).unwrap();
        assert_eq!(budget.estimated_input, 12_000);
        assert_eq!(budget.output_budget, 32_000);
        assert_eq!(clamp_max_tokens(Some(&budget), 32_000), Some(32_000));
    }

    #[test]
    fn allows_large_max_tokens_when_budget_suffices() {
        // 用户实际场景：剩余 18819，预计输入 5000，客户端 max_tokens 32000。
        let budget = decide_quota(Some(18_819), 5_000, 32_000).unwrap();
        assert_eq!(budget.output_budget, 13_819);
        assert_eq!(clamp_max_tokens(Some(&budget), 32_000), Some(13_819));
    }

    #[test]
    fn rejects_when_remaining_cannot_cover_input() {
        let error = decide_quota(Some(500), 12_000, 32_000).unwrap_err();
        assert!(error.contains("Quota insufficient"));
    }

    #[test]
    fn rejects_exhausted_quota() {
        let error = decide_quota(Some(0), 100, 1000).unwrap_err();
        assert!(error.starts_with("Quota exceeded"));
    }

    #[test]
    fn near_exhaustion_gets_small_output_budget() {
        let budget = decide_quota(Some(20_000), 19_000, 32_000).unwrap();
        assert_eq!(budget.output_budget, 1_000);
        assert_eq!(clamp_max_tokens(Some(&budget), 32_000), Some(1_000));
    }

    #[test]
    fn unspecified_max_tokens_uses_available_budget() {
        let budget = decide_quota(Some(10_000), 4_000, 0).unwrap();
        assert_eq!(budget.output_budget, 6_000);
        assert_eq!(clamp_max_tokens(Some(&budget), 0), None);
    }

    #[test]
    fn remaining_only_for_limited_keys() {
        assert_eq!(remaining(-1, 100), None);
        assert_eq!(remaining(20_000, 1_181), Some(18_819));
    }
}
