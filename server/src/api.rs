use axum::extract::{ConnectInfo, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use aipet_core::PetCreationRequest;
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::audit::{ClientInfo, RunRecord};
use crate::state::{AppState, StartError, StatusResponse, TaskSnapshot};

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/status", get(status))
        .route("/history", get(history))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task))
        .route("/tasks/{id}/confirm-base", post(confirm_base))
        .route("/tasks/{id}/cancel", post(cancel_task))
        .route("/tasks/{id}/base-image", get(base_image))
        .route("/tasks/{id}/artifact", get(artifact))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true }))
}

async fn status(State(state): State<Arc<AppState>>) -> Json<StatusResponse> {
    Json(state.status().await)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateTaskResponse {
    task_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BusyBody {
    error: String,
    task_id: String,
    pet_name: String,
}

fn client_info(addr: SocketAddr, headers: &HeaderMap) -> ClientInfo {
    let forwarded = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .filter(|s| !s.is_empty());
    let ip = forwarded
        .clone()
        .unwrap_or_else(|| addr.ip().to_string());
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    ClientInfo {
        ip,
        user_agent,
        forwarded_for: forwarded,
    }
}

async fn create_task(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<PetCreationRequest>,
) -> Response {
    let client = client_info(addr, &headers);
    state.audit.server_log_line(
        "info",
        &format!(
            "POST /tasks from {} pet=\"{}\" style={}",
            client.ip, request.pet_name, request.style_preset
        ),
    );

    if request.pet_name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "宠物名称不能为空" })),
        )
            .into_response();
    }
    if request.description.trim().is_empty()
        && request
            .reference_image
            .as_ref()
            .map(|s| s.is_empty())
            .unwrap_or(true)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "请提供描述或参考图" })),
        )
            .into_response();
    }

    match state.start_task(request, client).await {
        Ok(task_id) => (StatusCode::OK, Json(CreateTaskResponse { task_id })).into_response(),
        Err(StartError::Busy { task_id, pet_name }) => (
            StatusCode::CONFLICT,
            Json(BusyBody {
                error: "服务端正在生成中".into(),
                task_id,
                pet_name,
            }),
        )
            .into_response(),
        Err(StartError::Config(msg)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response(),
    }
}

async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<TaskSnapshot>, (StatusCode, Json<serde_json::Value>)> {
    state.get_task(&id).await.map(Json).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "任务不存在" })),
        )
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmBody {
    confirmed: bool,
}

async fn confirm_base(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    Json(body): Json<ConfirmBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state.audit.server_log_line(
        "info",
        &format!(
            "POST /tasks/{id}/confirm-base confirmed={} from {}",
            body.confirmed,
            addr.ip()
        ),
    );
    state
        .confirm_base(&id, body.confirmed)
        .await
        .map(|_| Json(serde_json::json!({ "ok": true })))
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": e }))))
}

async fn cancel_task(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state.audit.server_log_line(
        "warn",
        &format!("POST /tasks/{id}/cancel from {}", addr.ip()),
    );
    state
        .cancel_task(&id)
        .await
        .map(|_| Json(serde_json::json!({ "ok": true })))
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": e }))))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BaseImageResponse {
    task_id: String,
    base_image_b64: String,
}

async fn base_image(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<BaseImageResponse>, (StatusCode, Json<serde_json::Value>)> {
    let task = state.get_task(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "任务不存在" })),
        )
    })?;
    let b64 = task.base_image_b64.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "基础形象尚未就绪" })),
        )
    })?;
    Ok(Json(BaseImageResponse {
        task_id: id,
        base_image_b64: b64,
    }))
}

async fn artifact(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    state.audit.server_log_line(
        "info",
        &format!("GET /tasks/{id}/artifact from {}", addr.ip()),
    );
    let (path, sha) = state.artifact_path(&id).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e })),
        )
    })?;
    let bytes = tokio::fs::read(&path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("读取产物失败: {e}") })),
        )
    })?;
    let mut response = Response::new(bytes.into());
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/zip"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_str(&format!("attachment; filename=\"{id}.zip\""))
            .unwrap_or_else(|_| header::HeaderValue::from_static("attachment")),
    );
    if let Some(sha) = sha {
        if let Ok(v) = header::HeaderValue::from_str(&sha) {
            response.headers_mut().insert("x-aipet-sha256", v);
        }
    }
    Ok(response)
}

#[derive(serde::Deserialize)]
struct HistoryQuery {
    #[serde(default = "default_history_limit")]
    limit: usize,
}

fn default_history_limit() -> usize {
    50
}

async fn history(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<HistoryQuery>,
) -> Result<Json<Vec<RunRecord>>, (StatusCode, Json<serde_json::Value>)> {
    let limit = q.limit.clamp(1, 200);
    state
        .audit
        .list_history(limit)
        .map(Json)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e })),
            )
        })
}
