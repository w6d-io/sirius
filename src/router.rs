use std::{fmt::Display, sync::Arc};

use anyhow::anyhow;
use axum::{extract::State, http::HeaderMap, http::StatusCode, response::Result, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use serde_email::Email;
use serde_json::Value;
use tokio::sync::RwLock;
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
    utils::error::send_error,
};

/// Enum representing  the type of id to use to get the kratos identity.
#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum IDType {
    ID(Uuid),
    Email(Email),
}

impl Display for IDType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IDType::Email(ref id) => write!(f, "{}", id.as_str()),
            IDType::ID(ref id) => write!(f, "{id}"),
        }
    }
}

/// Structure representing the payload data.
#[derive(Deserialize, Clone, Debug)]
pub struct Data {
    //id :can be the email or the uuid depending of the endpoint
    pub id: IDType,
    #[serde(rename(deserialize = "type"))]
    pub ressource_type: String,
    pub ressource_id: String,
    pub value: Value,
}

async fn update_organisation_handler(
    config: Arc<SiriusConfig>,
    cookies: CookieJar,
    payload: Vec<Data>,
    correlation_id: &str,
) -> Result<(), RouterError> {
    let Some(kratos_cookie) = cookies.get("ory_kratos_session") else {
        error!("Kratos cookie not found");
        return Err(RouterError::Status(StatusCode::UNAUTHORIZED));
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
            users.push((data.ressource_id.clone(), data.value.clone()));
        }
    }
    let identity = update_controller(
        config.clone(),
        payload,
        identity,
        "organisation",
        correlation_id,
    )
    .await?;
    if !users.is_empty() {
        info!("updating group!");
        sync_groups(config.clone(), &identity, &users).await?;
        let mode = SyncMode::User(users);
        info!("updating user!");
        sync(&config, identity, mode).await?;
    }
    Ok(())
}

/// This route is used to update the groups of an organization them sync the groups and the users.
#[tracing::instrument]
#[axum_macros::debug_handler]
pub async fn update_organisation(
    State(config): State<Arc<RwLock<SiriusConfig>>>,
    headers: HeaderMap,
    cookies: CookieJar,
    Json(payload): Json<Vec<Data>>,
) -> Result<&'static str, RouterError> {
    info!("new request!");
    let correlation_id = headers
        .get("correlation_id")
        .ok_or_else(|| anyhow!("the request as no correlation id!"))?
        .to_str()?;
    let config = config.read().await.clone();
    let config = Arc::new(config);
    if let Err(e) =
        update_organisation_handler(config.clone(), cookies, payload, correlation_id).await
    {
        send_error(&config.kafka, "error", &e, correlation_id).await?;
        return Err(e);
    }
    Ok("200")
}

async fn update_groups_handler(
    config: Arc<SiriusConfig>,
    cookies: CookieJar,
    payload: Vec<Data>,
    correlation_id: &str,
) -> Result<(), RouterError> {
    let Some(kratos_cookie) = cookies.get("ory_kratos_session") else {
        error!("Kratos cookie not found");
        return Err(RouterError::Status(StatusCode::UNAUTHORIZED));
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
            users.push((data.ressource_id.clone(), data.value.clone()));
        }
        if data.ressource_type == "project" {
            projects.push(data.ressource_id.clone());
        }
    }
    info!("users: {users:?}");
    info!("project: {projects:?}");
    let group =
        update_controller(config.clone(), payload, identity, "groups", correlation_id).await?;
    info!("group updated");
    if !users.is_empty() {
        let sync_mode = SyncMode::User(users);
        info!("lauching users sync");
        tokio::spawn(sync_user(
            config.clone(),
            group.clone(),
            correlation_id.to_owned(),
            sync_mode,
        ));
    }
    if !projects.is_empty() {
        let sync_mode = SyncMode::Project(projects);
        info!("lauching projects sync");
        tokio::spawn(sync_user(
            config,
            group,
            correlation_id.to_owned(),
            sync_mode,
        ));
    }
    Ok(())
}

/// This route is used to update a group then sync the users groups and projects.
#[tracing::instrument]
#[axum_macros::debug_handler]
pub async fn update_groups(
    State(config): State<Arc<RwLock<SiriusConfig>>>,
    headers: HeaderMap,
    cookies: CookieJar,
    Json(payload): Json<Vec<Data>>,
) -> Result<&'static str, RouterError> {
    info!("new request!");
    let correlation_id = headers
        .get("correlation_id")
        .ok_or_else(|| anyhow!("the request as no correlation id!"))?
        .to_str()?;
    let config = config.read().await.clone();
    let config = Arc::new(config);
    if let Err(e) = update_groups_handler(config.clone(), cookies, payload, correlation_id).await {
        send_error(&config.kafka, "error", &e, correlation_id).await?;
        return Err(e);
    }
    Ok("200")
}

async fn update_projects_handler(
    config: Arc<SiriusConfig>,
    cookies: CookieJar,
    payload: Vec<Data>,
    correlation_id: &str,
) -> Result<(), RouterError> {
    let Some(kratos_cookie) = cookies.get("ory_kratos_session") else {
        error!("kratos cookie not found");
        return Err(RouterError::Status(StatusCode::UNAUTHORIZED));
    };
    let identity = config
        .kratos
        .validate_session(kratos_cookie)
        .await
        .map_err(|_| RouterError::Status(StatusCode::UNAUTHORIZED))?;
    info!("identity validated");
    update_controller(config, payload, identity, "projects", correlation_id).await?;
    Ok(())
}

/// This route is used to update an user projects.
#[tracing::instrument]
#[axum_macros::debug_handler]
pub async fn update_projects(
    State(config): State<Arc<RwLock<SiriusConfig>>>,
    headers: HeaderMap,
    cookies: CookieJar,
    Json(payload): Json<Vec<Data>>,
) -> Result<&'static str, RouterError> {
    info!("new request!");
    let correlation_id = headers
        .get("correlation_id")
        .ok_or_else(|| anyhow!("the request as no correlation id!"))?
        .to_str()?;

    let config = config.read().await.clone();
    let config = Arc::new(config);
    if let Err(e) = update_projects_handler(config.clone(), cookies, payload, correlation_id).await
    {
        send_error(&config.kafka, "error", &e, correlation_id).await?;
        return Err(e);
    }
    Ok("200")
}

async fn list_projects_handler(
    config: &SiriusConfig,
    cookies: CookieJar,
) -> Result<String, RouterError> {
    let Some(kratos_cookie) = cookies.get("ory_kratos_session") else {
        error!("Kratos cookie not found");
        return Err(RouterError::Status(StatusCode::UNAUTHORIZED));
    };
    let identity = config.kratos.validate_session(kratos_cookie).await?;
    info!("identity validated");
    let data = list_project_controller(identity, config).await?;
    let resp = serde_json::to_string(&data)?;
    Ok(resp)
}

///This route list all the projects from an identity in the configured mode (public, admin or trait).
#[tracing::instrument]
#[axum_macros::debug_handler]
pub async fn list_projects(
    State(config): State<Arc<RwLock<SiriusConfig>>>,
    headers: HeaderMap,
    cookies: CookieJar,
) -> Result<String, RouterError> {
    info!("new request!");
    let correlation_id = headers
        .get("correlation_id")
        .ok_or_else(|| anyhow!("the request as no correlation id!"))?
        .to_str()?;

    let config = config.read().await.clone();
    let ret = list_projects_handler(&config, cookies).await;
    if let Err(ref e) = ret {
        send_error(&config.kafka, "error", e, correlation_id).await?;
    }
    ret
}

async fn list_groups_handler(
    config: &SiriusConfig,
    cookies: CookieJar,
) -> Result<String, RouterError> {
    let Some(kratos_cookie) = cookies.get("ory_kratos_session") else {
        error!("Kratos cookie not found");
        return Err(RouterError::Status(StatusCode::UNAUTHORIZED));
    };
    let identity = config.kratos.validate_session(kratos_cookie).await?;
    info!("identity validated");
    let data = list_controller(identity, "group", config).await?;
    let resp = serde_json::to_string(&data)?;
    Ok(resp)
}

///This route list the groups from an identity in the configured mode (public, admin or trait).
#[tracing::instrument]
#[axum_macros::debug_handler]
pub async fn list_groups(
    State(config): State<Arc<RwLock<SiriusConfig>>>,
    headers: HeaderMap,
    cookies: CookieJar,
) -> Result<String, RouterError> {
    info!("new request!");
    let correlation_id = headers
        .get("correlation_id")
        .ok_or_else(|| anyhow!("the request as no correlation id!"))?
        .to_str()?;

    let config = config.read().await.clone();
    let ret = list_groups_handler(&config, cookies).await;
    if let Err(ref e) = ret {
        send_error(&config.kafka, "error", e, correlation_id).await?;
    }
    ret
}

async fn list_orga_handler(
    config: &SiriusConfig,
    cookies: CookieJar,
) -> Result<String, RouterError> {
    let Some(kratos_cookie) = cookies.get("ory_kratos_session") else {
        error!("Kratos cookie not found");
        return Err(RouterError::Status(StatusCode::UNAUTHORIZED));
    };
    let identity = config.kratos.validate_session(kratos_cookie).await?;
    info!("identity validated");
    let data = list_controller(identity, "organisation", config).await?;
    let resp = serde_json::to_string(&data)?;
    Ok(resp)
}

///This route list all the organisations from an identity in the configured mode (public, admin or trait).
#[tracing::instrument]
#[axum_macros::debug_handler]
pub async fn list_orga(
    State(config): State<Arc<RwLock<SiriusConfig>>>,
    headers: HeaderMap,
    cookies: CookieJar,
) -> Result<String, RouterError> {
    info!("new request!");
    let correlation_id = headers
        .get("correlation_id")
        .ok_or_else(|| anyhow!("the request as no correlation id!"))?
        .to_str()?;

    let config = config.read().await.clone();
    let ret = list_orga_handler(&config, cookies).await;
    if let Err(ref e) = ret {
        send_error(&config.kafka, "error", e, correlation_id).await?;
    }
    ret
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
        utils::test::{configure, IDENTITY_GROUP, IDENTITY_ORG, IDENTITY_USER},
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
        let opa_server = Server::new_async().await;
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
        /* let opa_mock = opa_server
        .mock("post", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"true"#)
        .create_async()
        .await; */
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
        // opa_mock.assert_async().await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_update_group() {
        let mut kratos_server = Server::new_async().await;
        let opa_server = Server::new_async().await;
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
        /* let opa_mock = opa_server
        .mock("post", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"true"#)
        .create_async()
        .await; */
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
        // opa_mock.assert_async().await;
        println!("{:#?}", response);
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_update_orga() {
        let mut kratos_server = Server::new_async().await;
        let opa_server = Server::new_async().await;
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
        /* let opa_mock = opa_server
        .mock("post", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"true"#)
        .create_async()
        .await; */
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
        // opa_mock.assert_async().await;
        println!("{:#?}", response);
        assert_eq!(response.status(), StatusCode::OK);
    }
}
