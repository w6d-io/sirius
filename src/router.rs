use std::sync::Arc;

use anyhow::anyhow;
use axum::{extract::State, http::StatusCode, response::Result, Extension, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use tokio::sync::RwLock;
use tower_http::request_id::RequestId;
use tracing::error;

use crate::{config::SiriusConfig, controler::update_controler, error::RouterError};

#[derive(Deserialize)]
pub struct Input {
    pub email: String,
    pub ressource_type: String,
    pub id: String,
    pub role: String,
}

pub async fn update(
    State(config): State<Arc<RwLock<SiriusConfig>>>,
    request_id: Extension<RequestId>,
    cookies: CookieJar,
    Json(payload): Json<Vec<Input>>,
) -> Result<&'static str, RouterError> {
    let uuid = request_id.header_value().to_str()?;
    let config_read = config.read().await;
    let kratos_cookie = match cookies.get("ory_kratos_session") {
        Some(cookie) => cookie,
        None => {
            error!("{uuid}: kratos cookie not found");
            return Err(RouterError::Status(StatusCode::UNAUTHORIZED));
        }
    };
    let _identity = config_read.kratos.validate_session(kratos_cookie).await?;
    update_controler(&config_read, payload, uuid).await?;
    Ok("200")
}

pub async fn alive() -> Result<&'static str, RouterError> {
    Ok("200")
}

pub async fn ready(
    State(config): State<Arc<RwLock<SiriusConfig>>>,
) -> Result<&'static str, RouterError> {
    let config = config.read().await;
    let client = match &config.kratos.client {
        Some(client) => client,
        None => Err(anyhow!("Kratos client not initialized"))?,
    };
    let iam_service = &config.iam.service;
    let addr = iam_service.addr.to_owned()+ "/api/iam/ready:" + &iam_service.ports.main as &str;
    let response = client.client.get(addr).send().await?;
    response.error_for_status()?;
    Ok("200")
}
