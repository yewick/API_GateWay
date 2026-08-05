use crate::adaptor::ChannelConfig;
use crate::db::models::Channel;

pub struct Dispatcher;

impl Dispatcher {
    /// 根据优先级、权重、模型支持度，构建有序的故障转移队列
    pub fn select_channels(channels: &[Channel], requested_model: &str) -> Vec<Channel> {
        // ── 第一步：过滤候选渠道 ─────────────────────────────
        let mut candidates: Vec<Channel> = channels
            .iter()
            .filter(|c| {
                // 1. 必须是启用状态
                if c.status != 1 {
                    return false;
                }
                // 2. 模型匹配：models 列表为空表示支持所有模型
                let models: Vec<String> = serde_json::from_str(&c.models).unwrap_or_default();
                if models.is_empty() || models.iter().any(|m| m == requested_model) {
                    return true;
                }
                // 3. 模型映射的 key 也算支持（映射名是面向下游的模型名）
                let mapping: serde_json::Value = serde_json::from_str(&c.model_mapping)
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                if let Some(obj) = mapping.as_object() {
                    return obj.contains_key(requested_model);
                }
                false
            })
            .cloned()
            .collect();

        if candidates.is_empty() {
            return Vec::new();
        }

        // ── 第二步：按优先级降序、权重降序排序 ───────────────
        candidates.sort_by(|a, b| b.priority.cmp(&a.priority).then(b.weight.cmp(&a.weight)));

        // ── 第三步：同优先级组内做权重随机排序 ───────────────
        let mut ordered = Vec::with_capacity(candidates.len());
        let mut start = 0;

        while start < candidates.len() {
            let priority = candidates[start].priority;
            // 找到同优先级的分组边界 [start, end)
            let mut end = start;
            while end < candidates.len() && candidates[end].priority == priority {
                end += 1;
            }

            let mut group = candidates[start..end].to_vec();
            let mut rng = rand::rng();

            // 权重随机抽取（不放回），直到组内取完
            while !group.is_empty() {
                let total_weight: i64 = group.iter().map(|c| c.weight.max(0)).sum();
                let index = if total_weight > 0 {
                    // 在 [0, total_weight) 区间随机一个点
                    let mut point = rand::Rng::random_range(&mut rng, 0..total_weight);
                    let mut selected = 0;
                    // 轮盘赌选择：谁的区间覆盖了这个点就选谁
                    for (idx, channel) in group.iter().enumerate() {
                        point -= channel.weight.max(0);
                        if point < 0 {
                            selected = idx;
                            break;
                        }
                    }
                    selected
                } else {
                    0  // 全是 0 权重时取第一个
                };

                ordered.push(group.remove(index));
            }

            start = end;
        }

        ordered
    }

    pub fn channel_to_config(channel: &Channel) -> ChannelConfig {
        let models: Vec<String> = serde_json::from_str(&channel.models).unwrap_or_default();
        let model_mapping: serde_json::Value = serde_json::from_str(&channel.model_mapping)
            .unwrap_or(serde_json::Value::Object(Default::default()));
        let extra: serde_json::Value = serde_json::from_str(&channel.config)
            .unwrap_or(serde_json::Value::Object(Default::default()));

        ChannelConfig {
            base_url: channel.base_url.clone(),
            api_key: channel.api_key.clone(),
            models,
            model_mapping,
            extra,
        }
    }
}

/// 测试用例
#[cfg(test)]
mod tests {
    use super::*;

    fn mock_channel(name: &str, priority: i64, weight: i64, models: &str) -> Channel {
        Channel {
            id: name.to_string(),
            name: name.to_string(),
            channel_type: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            models: models.to_string(),
            status: 1,
            priority,
            weight,
            config: "{}".to_string(),
            model_mapping: "{}".to_string(),
            created_at: "2026-01-01".to_string(),
            updated_at: "2026-01-01".to_string(),
            last_test_at: None,
            last_test_ok: None,
        }
    }

    #[test]
    fn test_filter_by_model() {
        let channels = vec![
            mock_channel("a", 1, 1, r#"["gpt-4o"]"#),
            mock_channel("b", 1, 1, r#"["deepseek-chat"]"#),
        ];
        let selected = Dispatcher::select_channels(&channels, "gpt-4o");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "a");
    }

    #[test]
    fn test_priority_order() {
        let channels = vec![
            mock_channel("low", 1, 1, "[]"),
            mock_channel("high", 10, 1, "[]"),
        ];
        let selected = Dispatcher::select_channels(&channels, "any-model");
        assert_eq!(selected[0].name, "high");  // 高优先级在前
    }

    #[test]
    fn test_all_channels_in_queue() {
        let channels = vec![
            mock_channel("a", 1, 5, "[]"),
            mock_channel("b", 1, 3, "[]"),
            mock_channel("c", 1, 2, "[]"),
        ];
        let selected = Dispatcher::select_channels(&channels, "any");
        // 同优先级，全部进入队列（顺序随机但都有）
        assert_eq!(selected.len(), 3);
    }
}