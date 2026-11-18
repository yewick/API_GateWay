use axum::{Router, extract::DefaultBodyLimit, routing::{get, post}};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::sync::Arc;
use tauri::AppHandle;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::{Any, CorsLayer};
use crate::AppState;
use super::handlers::*;

pub fn create_router(app: AppHandle, state: Arc<AppState>) -> Router {
    let shared = SharedState { app: app.clone(), state: state.clone() };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any);

    // Service registry — 合并所有服务路由（知识库 / MCP 等）
    let registry = crate::services::ServiceRegistry::new();
    let service_router = registry.merge_routes(state.clone());

    Router::new()
        .route("/v1/chat/completions", post(handle_chat_completions))
        .route("/v1/completions", post(handle_completions))
        .route("/v1/responses", post(handle_responses))
        .route("/v1/embeddings", post(handle_embeddings))
        .route("/v1/models", get(handle_list_models))
        .route("/v1/messages", post(handle_messages))
        .route("/v1/images/generations", post(handle_images))
        .route("/v1/audio/transcriptions", post(handle_audio_transcriptions))
        .route("/v1/audio/speech", post(handle_audio_speech))
        .route("/health", get(handle_health))
        .merge(service_router)
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .layer(CatchPanicLayer::custom(handle_panic))
        .layer(cors)
        .with_state(shared)
}

#[derive(Clone)]
pub struct SharedState {
    pub app: AppHandle,
    pub state: Arc<AppState>,
}

/// 兜住 handler 执行期的 panic，返回 JSON 500，而不是让连接静默断开。
/// 注意：响应体流（SSE 已开始写出）之后的 panic 不在本层能力范围内，
/// 那类错误应由流式转换器发送终端事件（如 `response.failed`）后正常关闭。
fn handle_panic(payload: Box<dyn std::any::Any + Send + 'static>) -> Response {
    let detail = match payload.downcast::<String>() {
        Ok(s) => *s,
        Err(payload) => match payload.downcast::<&str>() {
            Ok(s) => s.to_string(),
            Err(_) => "unknown panic".to_string(),
        },
    };
    tracing::error!(panic = %detail, "HTTP handler panicked");

    let body = serde_json::json!({
        "error": {
            "message": "internal server error",
            "type": "server_error"
        }
    });
    (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    async fn panicking_handler() {
        panic!("synthetic panic");
    }

    #[tokio::test]
    async fn catch_panic_layer_returns_json_500() {
        let app = Router::new()
            .route("/panic", get(panicking_handler))
            .layer(CatchPanicLayer::custom(handle_panic));

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/panic")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error"]["type"], "server_error");
        assert_eq!(v["error"]["message"], "internal server error");
    }
}
