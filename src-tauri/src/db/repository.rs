use super::models::*;
use sqlx::SqlitePool;

pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn get_all_channels(&self) -> Result<Vec<Channel>, sqlx::Error> {
        sqlx::query_as::<_, Channel>(
            "SELECT * FROM channels ORDER BY priority DESC, created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_enabled_channels(&self) -> Result<Vec<Channel>, sqlx::Error> {
        sqlx::query_as::<_, Channel>(
            "SELECT * FROM channels WHERE status = 1 ORDER BY priority DESC, weight DESC",
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_channel(&self, id: &str) -> Result<Channel, sqlx::Error> {
        sqlx::query_as::<_, Channel>("SELECT * FROM channels WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
    }

    pub async fn get_api_key_by_key(&self, key: &str) -> Result<ApiKey, sqlx::Error> {
        sqlx::query_as::<_, ApiKey>("SELECT * FROM api_keys WHERE key = ? AND status = 1")
            .bind(key)
            .fetch_one(&self.pool)
            .await
    }

    pub async fn create_channel(
        &self,
        input: &CreateChannelInput,
    ) -> Result<Channel, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_iso();
        let models = serde_json::to_string(&input.models).unwrap_or_else(|_| "[]".to_string());
        let config = input
            .config
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|| "{}".to_string());
        let model_mapping = input
            .model_mapping
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|| "{}".to_string());

        sqlx::query(
            "INSERT INTO channels (id, name, type, base_url, api_key, models, status, priority, weight, config, model_mapping, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&input.channel_type)
        .bind(&input.base_url)
        .bind(&input.api_key)
        .bind(&models)
        .bind(input.priority.unwrap_or(0))
        .bind(input.weight.unwrap_or(1))
        .bind(&config)
        .bind(&model_mapping)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        self.get_channel(&id).await
    }

    pub async fn update_channel(
        &self,
        input: &UpdateChannelInput,
    ) -> Result<Channel, sqlx::Error> {
        let now = now_iso();
        let mut q = sqlx::QueryBuilder::new("UPDATE channels SET updated_at = ");
        q.push_bind(&now);

        if let Some(name) = &input.name {
            q.push(", name = ").push_bind(name);
        }
        if let Some(channel_type) = &input.channel_type {
            q.push(", type = ").push_bind(channel_type);
        }
        if let Some(base_url) = &input.base_url {
            q.push(", base_url = ").push_bind(base_url);
        }
        if let Some(api_key) = &input.api_key {
            q.push(", api_key = ").push_bind(api_key);
        }
        if let Some(models) = &input.models {
            let m = serde_json::to_string(models).unwrap_or_else(|_| "[]".to_string());
            q.push(", models = ").push_bind(m);
        }
        if let Some(status) = input.status {
            q.push(", status = ").push_bind(status);
        }
        if let Some(priority) = input.priority {
            q.push(", priority = ").push_bind(priority);
        }
        if let Some(weight) = input.weight {
            q.push(", weight = ").push_bind(weight);
        }
        if let Some(config) = &input.config {
            let c = serde_json::to_string(config).unwrap_or_else(|_| "{}".to_string());
            q.push(", config = ").push_bind(c);
        }
        if let Some(model_mapping) = &input.model_mapping {
            let mm =
                serde_json::to_string(model_mapping).unwrap_or_else(|_| "{}".to_string());
            q.push(", model_mapping = ").push_bind(mm);
        }

        q.push(" WHERE id = ").push_bind(&input.id);
        q.build().execute(&self.pool).await?;

        self.get_channel(&input.id).await
    }

    pub async fn search_logs(
        &self,
        keyword: Option<&str>,
        channel_name: Option<&str>,
        model: Option<&str>,
        mode: Option<&str>,
        status_code: Option<i64>,
        is_stream: Option<i64>,
        is_retry: Option<i64>,
        risk_level: Option<&str>,
        security_action: Option<&str>,
        finding_rule: Option<&str>,
        date_from: Option<&str>,
        date_to: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RequestLog>, sqlx::Error> {
        let mut q = sqlx::QueryBuilder::new("SELECT * FROM request_logs WHERE 1=1");

        if let Some(kw) = keyword {
            let pattern = format!("%{}%", kw);
            q.push(" AND (api_key_name LIKE ")
                .push_bind(pattern.clone())
                .push(" OR channel_name LIKE ")
                .push_bind(pattern.clone())
                .push(" OR model LIKE ")
                .push_bind(pattern)
                .push(")");
        }
        if let Some(cn) = channel_name {
            q.push(" AND channel_name = ").push_bind(cn);
        }
        if let Some(m) = model {
            q.push(" AND model = ").push_bind(m);
        }
        if let Some(mode_val) = mode {
            q.push(" AND mode = ").push_bind(mode_val);
        }
        if let Some(sc) = status_code {
            q.push(" AND status_code = ").push_bind(sc);
        }
        if let Some(s) = is_stream {
            q.push(" AND is_stream = ").push_bind(s);
        }
        if let Some(r) = is_retry {
            q.push(" AND is_retry = ").push_bind(r);
        }
        if let Some(rl) = risk_level {
            q.push(" AND risk_level = ").push_bind(rl);
        }
        if let Some(sa) = security_action {
            q.push(" AND security_action = ").push_bind(sa);
        }
        if let Some(fr) = finding_rule {
            q.push(" AND EXISTS (SELECT 1 FROM security_findings f WHERE f.log_id = request_logs.id AND f.rule LIKE ")
                .push_bind(format!("%{}%", fr))
                .push(")");
        }
        if let Some(from) = date_from {
            q.push(" AND created_at >= ").push_bind(from);
        }
        if let Some(to) = date_to {
            q.push(" AND created_at <= ").push_bind(to);
        }

        q.push(" ORDER BY created_at DESC LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);

        q.build_query_as::<RequestLog>()
            .fetch_all(&self.pool)
            .await
    }

    /// 获取单条日志
    pub async fn get_log(&self, id: &str) -> Result<RequestLog, sqlx::Error> {
        sqlx::query_as::<_, RequestLog>("SELECT * FROM request_logs WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
    }

    /// 删除单条日志（级联删除关联的 security_findings）
    pub async fn delete_log(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM security_findings WHERE log_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM request_logs WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 按天聚合请求统计（供用量页面）
    pub async fn get_log_stats(&self, days: i64) -> Result<Vec<LogStats>, sqlx::Error> {
        let cutoff = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(days))
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();
        sqlx::query_as::<_, LogStats>(
            "SELECT substr(created_at, 1, 10) AS date,
                    COUNT(*) AS requests,
                    COALESCE(SUM(total_tokens), 0) AS tokens
             FROM request_logs
             WHERE created_at >= ?
             GROUP BY date
             ORDER BY date ASC",
        )
        .bind(&cutoff)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_dashboard_stats(&self) -> Result<DashboardStats, sqlx::Error> {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let today_prefix = format!("{}%", today);

        let today_requests: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM request_logs WHERE created_at LIKE ?",
        )
        .bind(&today_prefix)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        let today_total_tokens: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(total_tokens), 0) FROM request_logs WHERE created_at LIKE ?",
        )
        .bind(&today_prefix)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        let total_channels: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM channels")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let active_channels: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM channels WHERE status = 1")
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);

        let total_api_keys: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_keys")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let total_requests: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_logs")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let total_tokens: i64 =
            sqlx::query_scalar("SELECT COALESCE(SUM(total_tokens), 0) FROM request_logs")
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);

        let avg_latency_ms: i64 = sqlx::query_scalar(
            "SELECT COALESCE(CAST(AVG(duration_ms) AS INTEGER), 0) FROM request_logs WHERE created_at LIKE ?",
        )
        .bind(&today_prefix)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        Ok(DashboardStats {
            today_requests,
            today_total_tokens,
            active_channels,
            avg_latency_ms,
            total_channels,
            total_api_keys,
            total_requests,
            total_tokens,
        })
    }

    pub async fn update_channel_test_result(
        &self,
        id: &str,
        success: bool,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        sqlx::query(
            "UPDATE channels SET last_test_at = ?, last_test_ok = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&now)
        .bind(success as i64)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 写入一条请求日志
    pub async fn create_log(&self, log: &RequestLog) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO request_logs
               (id, seq, api_key_id, api_key_name, channel_id, channel_name, model, upstream_model,
                mode, status_code, prompt_tokens, completion_tokens, total_tokens, duration_ms,
                error_message, is_stream, is_retry, created_at, request_body,
                risk_level, risk_score, risk_summary, security_action, sanitized, blocked_reason)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&log.id)
        .bind(log.seq)
        .bind(&log.api_key_id)
        .bind(&log.api_key_name)
        .bind(&log.channel_id)
        .bind(&log.channel_name)
        .bind(&log.model)
        .bind(&log.upstream_model)
        .bind(&log.mode)
        .bind(log.status_code)
        .bind(log.prompt_tokens)
        .bind(log.completion_tokens)
        .bind(log.total_tokens)
        .bind(log.duration_ms)
        .bind(&log.error_message)
        .bind(log.is_stream)
        .bind(log.is_retry)
        .bind(&log.created_at)
        .bind(&log.request_body)
        .bind(&log.risk_level)
        .bind(log.risk_score)
        .bind(&log.risk_summary)
        .bind(&log.security_action)
        .bind(log.sanitized)
        .bind(&log.blocked_reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 写入安全风险明细
    pub async fn create_security_findings(
        &self,
        log_id: &str,
        findings: &[crate::security::SecurityFinding],
        action: &str,
    ) -> Result<(), sqlx::Error> {
        let now = now_iso();
        for f in findings {
            sqlx::query(
                "INSERT INTO security_findings (log_id, rule, severity, detail, action, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(log_id)
            .bind(&f.rule_id)
            .bind(&f.severity.as_str())
            .bind(&f.description)
            .bind(action)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// 配额扣减：增加已用 token
    pub async fn increment_quota(
        &self,
        api_key_id: &str,
        tokens: i64,
    ) -> Result<(), sqlx::Error> {
        let now = now_iso();
        sqlx::query(
            "UPDATE api_keys SET quota_used = quota_used + ?, updated_at = ? WHERE id = ?",
        )
        .bind(tokens)
        .bind(&now)
        .bind(api_key_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ===== API Key CRUD =====

    /// 生成 sk-yeapi-{32位hex} 格式密钥
    fn generate_api_key() -> String {
        format!("sk-yeapi-{}", uuid::Uuid::new_v4().simple())
    }

    /// 列出全部密钥（新在前）
    pub async fn get_all_api_keys(&self) -> Result<Vec<ApiKey>, sqlx::Error> {
        sqlx::query_as::<_, ApiKey>(
            "SELECT * FROM api_keys ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
    }

    /// 创建密钥（生成 sk-yeapi-* 格式，默认启用）
    pub async fn create_api_key(
        &self,
        input: &CreateApiKeyInput,
    ) -> Result<ApiKey, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let key = Self::generate_api_key();
        let now = now_iso();
        sqlx::query(
            "INSERT INTO api_keys
               (id, name, key, status, allowed_models, allowed_channels,
                quota_limit, quota_used, expires_at, created_at, updated_at)
             VALUES (?, ?, ?, 1, ?, ?, ?, 0, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(&key)
        .bind(serde_json::to_string(&input.allowed_models).unwrap_or_else(|_| "[]".to_string()))
        .bind(serde_json::to_string(&input.allowed_channels).unwrap_or_else(|_| "[]".to_string()))
        .bind(input.quota_limit.unwrap_or(-1))
        .bind(&input.expires_at)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        self.get_api_key_by_key(&key).await
    }

    /// 启用/禁用密钥
    pub async fn update_api_key(&self, id: &str, status: i64) -> Result<(), sqlx::Error> {
        let now = now_iso();
        sqlx::query("UPDATE api_keys SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status)
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 删除密钥
    pub async fn delete_api_key(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM api_keys WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 删除渠道
    pub async fn delete_channel(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM channels WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 获取单条日志的所有安全发现明细
    pub async fn get_findings_by_log_id(
        &self,
        log_id: &str,
    ) -> Result<Vec<SecurityFindingRow>, sqlx::Error> {
        sqlx::query_as::<_, SecurityFindingRow>(
            "SELECT * FROM security_findings WHERE log_id = ? ORDER BY severity DESC, id ASC",
        )
        .bind(log_id)
        .fetch_all(&self.pool)
        .await
    }

    /// 获取所有启用的自定义规则（用于扫描时加载）
    pub async fn get_enabled_custom_rules(
        &self,
    ) -> Result<Vec<crate::security::CustomRule>, sqlx::Error> {
        sqlx::query_as::<_, crate::security::CustomRule>(
            "SELECT * FROM security_custom_rules WHERE enabled = 1 ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
    }

    /// 获取全部内置规则（用于扫描时构建禁用集合）
    pub async fn get_all_builtin_rules(
        &self,
    ) -> Result<Vec<crate::security::BuiltinRule>, sqlx::Error> {
        sqlx::query_as::<_, crate::security::BuiltinRule>(
            "SELECT * FROM security_builtin_rules ORDER BY rule_id",
        )
        .fetch_all(&self.pool)
        .await
    }
}
