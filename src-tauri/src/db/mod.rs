pub mod models;
pub mod repository;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use tauri::{AppHandle, Manager};

pub struct Database {
    pub pool: SqlitePool,
}

impl Database {
    pub async fn new(app: &AppHandle) -> Self {
        // 获取应用数据目录（macOS: ~/Library/Application Support/com.yeapi.app）
        let app_data_dir = app
            .path()
            .app_data_dir()
            .expect("failed to get app data dir");

        std::fs::create_dir_all(&app_data_dir).expect("failed to create app data dir");

        // mode=rwc：不存在则创建
        let db_path = app_data_dir.join("yeapi.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .expect("failed to connect to database");

        // 执行 migrations（编译时嵌入 SQL 文件）
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .ok();

        Self { pool }
    }
}
