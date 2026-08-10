use crate::AppState;
use crate::db::models::DashboardStats;
use crate::db::repository::Repository;

#[tauri::command]
pub async fn get_dashboard_stats(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<DashboardStats, String> {
    let repo = Repository::new(state.db.pool.clone());
    repo.get_dashboard_stats().await.map_err(|e| e.to_string())
}
