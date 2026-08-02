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

        Ok(DashboardStats {
            today_requests,
            today_total_tokens,
            total_channels,
            active_channels,
        })
    }
}
