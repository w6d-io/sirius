use std::{fmt::Display, sync::Arc};

use anyhow::anyhow;
use axum::{extract::State, http::StatusCode, response::Result, Extension, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use serde_email::Email;
use serde_json::Value;
use tokio::sync::RwLock;
use tower_http::request_id::RequestId;
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    config::SiriusConfig,
    controller::{
        list::{list_controller, list_project_controller},
        sync::{sync, sync_groups, sync_user, SyncMode},
        update::update_controller,
    },
    error::RouterError,
};

#[derive(Deserialize, Clone)]
#[serde(untagged)]
pub enum IDType {
    ID(Uuid),
    Email(Email),
}

impl Display for IDType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IDType::Email(ref id) => write!(f, "{}", id.as_str()),
            IDType::ID(ref id) => write!(f, "{}", id),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct Data {
    //id :can be the email or the uuid depending of the endpoint
    pub id: IDType,
    #[serde(rename(deserialize = "type"))]
    pub ressource_type: String,
    pub ressource_id: String,
    pub value: Value,
}

pub async fn update_organisaion(
    State(config): State<Arc<RwLock<SiriusConfig>>>,
    request_id: Extension<RequestId>,
    cookies: CookieJar,
    Json(payload): Json<Vec<Data>>,
) -> Result<&'static str, RouterError> {
    info!("new request!");
    let request_id = request_id.header_value().to_str()?;
    let config = config.read().await.clone();
    let config = Arc::new(config);
    let kratos_cookie = match cookies.get("ory_kratos_session") {
        Some(cookie) => cookie,
        None => {
            error!("{request_id}: kratos cookie not found");
            return Err(RouterError::Status(StatusCode::UNAUTHORIZED));
        }
    };
    let identity = config
        .kratos
        .validate_session(kratos_cookie)
        .await
        .map_err(|_| RouterError::Status(StatusCode::UNAUTHORIZED))?;
    info!("identity validated");
    let mut users = Vec::new();
    for data in &payload {
        if data.ressource_type == "user" {
            users.push((data.ressource_id.to_owned(), data.value.clone()));
        }
    }
    let identity = update_controller(config.clone(), payload, request_id, identity).await?;
    if !users.is_empty() {
        println!("updating group!");
        sync_groups(config.clone(), &identity, request_id, &users).await?;
        let mode = SyncMode::User(users);
        println!("updating user!");
        sync(config, identity, request_id, mode).await?;
    }
    Ok("200")
}

pub async fn update_groups(
    State(config): State<Arc<RwLock<SiriusConfig>>>,
    request_id: Extension<RequestId>,
    cookies: CookieJar,
    Json(payload): Json<Vec<Data>>,
) -> Result<&'static str, RouterError> {
    info!("new request!");
    let request_id = request_id.header_value().to_str()?.to_owned();
    let config = config.read().await.clone();
    let config = Arc::new(config);
    let kratos_cookie = match cookies.get("ory_kratos_session") {
        Some(cookie) => cookie,
        None => {
            error!("{request_id}: kratos cookie not found");
            return Err(RouterError::Status(StatusCode::UNAUTHORIZED));
        }
    };
    let identity = config
        .kratos
        .validate_session(kratos_cookie)
        .await
        .map_err(|_| RouterError::Status(StatusCode::UNAUTHORIZED))?;
    info!("identity validated");
    let mut users = Vec::new();
    let mut projects = Vec::new();
    for data in &payload {
        if data.ressource_type == "user" {
            users.push((data.ressource_id.to_owned(), data.value.clone()));
        }
        if data.ressource_type == "project" {
            projects.push(data.ressource_id.to_owned());
        }
    }
    let group = update_controller(config.clone(), payload, &request_id, identity).await?;
    info!("group updated");
    let sync_mode = if !users.is_empty() {
        SyncMode::User(users)
    } else {
        SyncMode::Project(projects)
    };
    info!("lauching user sync");
    tokio::spawn(sync_user(config, group, request_id.clone(), sync_mode));
    Ok("200")
}

pub async fn update_projects(
    State(config): State<Arc<RwLock<SiriusConfig>>>,
    request_id: Extension<RequestId>,
    cookies: CookieJar,
    Json(payload): Json<Vec<Data>>,
) -> Result<&'static str, RouterError> {
    info!("new request!");
    let request_id = request_id.header_value().to_str()?;
    let config = config.read().await.clone();
    let config = Arc::new(config);
    let kratos_cookie = match cookies.get("ory_kratos_session") {
        Some(cookie) => cookie,
        None => {
            error!("{request_id}: kratos cookie not found");
            return Err(RouterError::Status(StatusCode::UNAUTHORIZED));
        }
    };
    let identity = config
        .kratos
        .validate_session(kratos_cookie)
        .await
        .map_err(|_| RouterError::Status(StatusCode::UNAUTHORIZED))?;
    info!("identity validated");
    update_controller(config, payload, request_id, identity).await?;
    Ok("200")
}

pub async fn list_projects(
    State(config): State<Arc<RwLock<SiriusConfig>>>,
    request_id: Extension<RequestId>,
    cookies: CookieJar,
) -> Result<String, RouterError> {
    info!("new request!");
    let request_id = request_id.header_value().to_str()?;
    let config = config.read().await.clone();
    let kratos_cookie = match cookies.get("ory_kratos_session") {
        Some(cookie) => cookie,
        None => {
            error!("{request_id}: kratos cookie not found");
            return Err(RouterError::Status(StatusCode::UNAUTHORIZED));
        }
    };
    let identity = config.kratos.validate_session(kratos_cookie).await?;
    info!("identity validated");
    let data = list_project_controller(request_id, identity, config).await?;
    let resp = serde_json::to_string(&data)?;

    Ok(resp)
}

pub async fn list_groups(
    State(config): State<Arc<RwLock<SiriusConfig>>>,
    request_id: Extension<RequestId>,
    cookies: CookieJar,
) -> Result<String, RouterError> {
    info!("new request!");
    let request_id = request_id.header_value().to_str()?;
    let config = config.read().await.clone();
    let kratos_cookie = match cookies.get("ory_kratos_session") {
        Some(cookie) => cookie,
        None => {
            error!("{request_id}: kratos cookie not found");
            return Err(RouterError::Status(StatusCode::UNAUTHORIZED));
        }
    };
    let identity = config.kratos.validate_session(kratos_cookie).await?;
    info!("identity validated");
    let data = list_controller(request_id, identity, "group", config).await?;
    let resp = serde_json::to_string(&data)?;

    Ok(resp)
}

pub async fn list_orga(
    State(config): State<Arc<RwLock<SiriusConfig>>>,
    request_id: Extension<RequestId>,
    cookies: CookieJar,
) -> Result<String, RouterError> {
    info!("new request!");
    let request_id = request_id.header_value().to_str()?;
    let config = config.read().await.clone();
    let kratos_cookie = match cookies.get("ory_kratos_session") {
        Some(cookie) => cookie,
        None => {
            error!("{request_id}: kratos cookie not found");
            return Err(RouterError::Status(StatusCode::UNAUTHORIZED));
        }
    };
    let identity = config.kratos.validate_session(kratos_cookie).await?;
    info!("identity validated");
    let data = list_controller(request_id, identity, "organisation", config).await?;
    let resp = serde_json::to_string(&data)?;

    Ok(resp)
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
    let addr = format!(
        "http://{}:{}/api/iam/ready",
        iam_service.addr, iam_service.ports.health
    );
    let response = client.client.get(addr).send().await?;
    response.error_for_status()?;
    Ok("200")
}

#[cfg(test)]
mod http_router_test {
    use std::sync::Arc;

    use axum::{
        body::Body,
        http::{header, Method, Request, StatusCode},
    };
    use mockito::Server;
    use ory_kratos_client::models::Session;
    use serde_json::json;
    use tokio::sync::RwLock;
    use tower::ServiceExt;

    use crate::{
        app, health,
        utils::test::{configure, IDENTITY_ORG, IDENTITY_GROUP, IDENTITY_USER},
    };

    #[tokio::test]
    async fn test_alive() {
        let config = configure(None, None, None).await;
        let config = Arc::new(RwLock::new(config));
        let app = health(config);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/alive")
                    .body(Body::from("200"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    #[tokio::test]
    async fn test_ready() {
        let mut iam_server = Server::new_async().await;

        let config = configure(None, None, Some(&iam_server)).await;
        let iam_mock = iam_server
            .mock("get", "/api/iam/ready")
            .with_status(200)
            .create_async()
            .await;
        let config = Arc::new(RwLock::new(config));
        let app = health(config);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/ready")
                    .header(header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                    .header("Cookie", "ory_kratos_session=bonjour")
                    .body(Body::from("200"))
                    .unwrap(),
            )
            .await
            .unwrap();
        iam_mock.assert_async().await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_update_users() {
        let mut kratos_server = Server::new_async().await;
        let mut opa_server = Server::new_async().await;
        let config = configure(Some(&kratos_server), Some(&opa_server), None).await;
        let body = "[".to_owned() + IDENTITY_USER + "]";
        let session = Session::new(
            "bonjour".to_owned(),
            serde_json::from_str(IDENTITY_USER).unwrap(),
        );
        let kratos_mock_admin = kratos_server
            .mock(
                "get",
                "/admin/identities?credentials_identifier=lol.lol@lol.io",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;
        let kratos_mock_session = kratos_server
            .mock("get", "/sessions/whoami")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&session).unwrap())
            .create_async()
            .await;
        let opa_mock = opa_server
            .mock("post", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"true"#)
            .create_async()
            .await;
        let config = Arc::new(RwLock::new(config));
        let app = app(config);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/iam/project")
                    .header(header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                    .header("Cookie", "ory_kratos_session=bonjour")
                    .body(Body::from(
                        serde_json::to_string(&json!([{
                          "id": "lol.lol@lol.io",
                          "type": "project",
                          "ressource_id": "222",
                          "value": ["contributor"]
                        }]))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        kratos_mock_session.assert_async().await;
        kratos_mock_admin.assert_async().await;
        opa_mock.assert_async().await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_update_group() {
        let mut kratos_server = Server::new_async().await;
        let mut opa_server = Server::new_async().await;
        let config = configure(Some(&kratos_server), Some(&opa_server), None).await;
        let session = Session::new(
            "bonjour".to_owned(),
            serde_json::from_str(IDENTITY_USER).unwrap(),
        );
        let kratos_mock_admin = kratos_server
            .mock(
                "get",
                "/admin/identities/9f425a8d-7efc-4768-8f23-7647a74fdf13",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(IDENTITY_GROUP)
            .create_async()
            .await;
        let kratos_mock_session = kratos_server
            .mock("get", "/sessions/whoami")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&session).unwrap())
            .create_async()
            .await;
        let opa_mock = opa_server
            .mock("post", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"true"#)
            .create_async()
            .await;
        let config = Arc::new(RwLock::new(config));
        let app = app(config);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/iam/group")
                    .header(header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                    .header("Cookie", "ory_kratos_session=bonjour")
                    .body(Body::from(
                        serde_json::to_string(&json!([{
                            //uuid from kratos exemple
                          "id": "9f425a8d-7efc-4768-8f23-7647a74fdf13",
                          "type": "user",
                          "ressource_id": "222",
                          "value": ["contributor"]
                        }]))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        kratos_mock_session.assert_async().await;
        kratos_mock_admin.assert_async().await;
        opa_mock.assert_async().await;
        println!("{:#?}", response);
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_update_orga() {
        let mut kratos_server = Server::new_async().await;
        let mut opa_server = Server::new_async().await;
        let config = configure(Some(&kratos_server), Some(&opa_server), None).await;
        let session = Session::new(
            "bonjour".to_owned(),
            serde_json::from_str(IDENTITY_USER).unwrap(),
        );
        let kratos_mock_admin = kratos_server
            .mock(
                "get",
                "/admin/identities/9f425a8d-7efc-4768-8f23-7647a74fdf13",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(IDENTITY_ORG)
            .create_async()
            .await;
        let kratos_mock_session = kratos_server
            .mock("get", "/sessions/whoami")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&session).unwrap())
            .create_async()
            .await;
        let opa_mock = opa_server
            .mock("post", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"true"#)
            .create_async()
            .await;
        let config = Arc::new(RwLock::new(config));
        let app = app(config);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/iam/organisation")
                    .header(header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                    .header("Cookie", "ory_kratos_session=bonjour")
                    .body(Body::from(
                        serde_json::to_string(&json!([{
                            //uuid from kratos exemple
                          "id": "9f425a8d-7efc-4768-8f23-7647a74fdf13",
                          "type": "user",
                          "ressource_id": "222",
                          "value": ["contributor"]
                        }]))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        kratos_mock_session.assert_async().await;
        kratos_mock_admin.assert_async().await;
        opa_mock.assert_async().await;
        println!("{:#?}", response);
        assert_eq!(response.status(), StatusCode::OK);
    }
}
