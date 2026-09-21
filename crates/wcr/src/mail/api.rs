//! SPDX-License-Identifier: Apache-2.0
//! Local HTTP for the desktop Email tab. Mounted beside `/status`.

use super::store::{CopySettings, MailRow, WaitHeader};
use super::MailHandle;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

pub type MailSlot = Arc<parking_lot::Mutex<Option<MailHandle>>>;

pub fn router(slot: MailSlot) -> Router {
    Router::new()
        .route("/mail/tab", get(tab))
        .route("/mail/list", get(list))
        .route("/mail/open", get(open_one))
        .route("/mail/waiting", get(waiting))
        .route("/mail/copy", get(copy_get))
        .route("/mail/compose", post(compose))
        .route("/mail/delete", post(delete_one))
        .route("/mail/check", post(check))
        .route("/mail/get", post(get_msgs))
        .route("/mail/copy", post(copy_set))
        .route("/mail/copy/confirm", post(copy_confirm))
        .layer(axum::Extension(slot))
}

async fn handle(slot: &MailSlot) -> std::result::Result<MailHandle, StatusCode> {
    slot.lock().clone().ok_or(StatusCode::SERVICE_UNAVAILABLE)
}

async fn tab(axum::Extension(slot): axum::Extension<MailSlot>) -> Result<Json<Value>, StatusCode> {
    let h = handle(&slot).await?;
    Ok(Json(h.tab_json()))
}

#[derive(Deserialize)]
struct FolderQ {
    folder: String,
}

async fn list(
    axum::Extension(slot): axum::Extension<MailSlot>,
    Query(q): Query<FolderQ>,
) -> Result<Json<Vec<MailRow>>, StatusCode> {
    let h = handle(&slot).await?;
    h.store.list(&q.folder).map(Json).map_err(store_err)
}

#[derive(Deserialize)]
struct IdQ {
    id: String,
}

async fn open_one(
    axum::Extension(slot): axum::Extension<MailSlot>,
    Query(q): Query<IdQ>,
) -> Result<Json<MailRow>, StatusCode> {
    let h = handle(&slot).await?;
    let _ = h.store.mark_read(&q.id);
    match h.store.get(&q.id) {
        Ok(Some(row)) => Ok(Json(row)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn waiting(
    axum::Extension(slot): axum::Extension<MailSlot>,
) -> Result<Json<Vec<WaitHeader>>, StatusCode> {
    let h = handle(&slot).await?;
    h.store.waiting().map(Json).map_err(store_err)
}

async fn copy_get(
    axum::Extension(slot): axum::Extension<MailSlot>,
) -> Result<Json<CopySettings>, StatusCode> {
    let h = handle(&slot).await?;
    let call = h.cfg.lock().callsign.clone();
    h.store.copy(&call).map(Json).map_err(store_err)
}

#[derive(Deserialize)]
struct ComposeBody {
    to: String,
    subject: String,
    body: String,
}

async fn compose(
    axum::Extension(slot): axum::Extension<MailSlot>,
    Json(b): Json<ComposeBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let h = handle(&slot)
        .await
        .map_err(|s| (s, "mail starting".into()))?;
    match h.compose(&b.to, &b.subject, &b.body).await {
        Ok(v) => Ok(Json(v)),
        Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
    }
}

#[derive(Deserialize)]
struct IdBody {
    id: String,
}

async fn delete_one(
    axum::Extension(slot): axum::Extension<MailSlot>,
    Json(b): Json<IdBody>,
) -> Result<Json<Value>, StatusCode> {
    let h = handle(&slot).await?;
    let ok = h.store.delete(&b.id).map_err(store_err)?;
    Ok(Json(json!({ "ok": ok })))
}

async fn check(
    axum::Extension(slot): axum::Extension<MailSlot>,
) -> Result<Json<Value>, StatusCode> {
    let h = handle(&slot).await?;
    h.check_list().await.map_err(store_err)?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct GetBody {
    ids: Vec<String>,
}

async fn get_msgs(
    axum::Extension(slot): axum::Extension<MailSlot>,
    Json(b): Json<GetBody>,
) -> Result<Json<Value>, StatusCode> {
    let h = handle(&slot).await?;
    h.check_get(b.ids).await.map_err(store_err)?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct CopyBody {
    address: String,
}

async fn copy_set(
    axum::Extension(slot): axum::Extension<MailSlot>,
    Json(b): Json<CopyBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let h = handle(&slot)
        .await
        .map_err(|s| (s, "mail starting".into()))?;
    h.set_copy(&b.address)
        .await
        .map(|()| Json(json!({ "ok": true, "pending": true })))
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

#[derive(Deserialize)]
struct CodeBody {
    code: String,
}

async fn copy_confirm(
    axum::Extension(slot): axum::Extension<MailSlot>,
    Json(b): Json<CodeBody>,
) -> Result<Json<Value>, StatusCode> {
    let h = handle(&slot).await?;
    let call = h.cfg.lock().callsign.clone();
    let ok = h
        .store
        .confirm_copy(&call, b.code.trim())
        .map_err(store_err)?;
    Ok(Json(json!({ "ok": ok })))
}

fn store_err(_: crate::error::Error) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}
