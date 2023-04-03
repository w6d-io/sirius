use std::{collections::BTreeSet, sync::Arc};

use anyhow::{anyhow, bail, Result};
use ory_kratos_client::{
    apis::{configuration::Configuration, identity_api::get_identity},
    models::Identity,
};
use tokio::task::JoinSet;
use tonic::Request;
use tracing::{debug, error};

use crate::{config::SiriusConfig, permission::Input, router::Data, utils::opa::validate_roles};

///get an identities form kratos by mail
async fn get_identity_by_mail(
    config: &SiriusConfig,
    data: &Data,
    request_id: &str,
) -> Result<Identity> {
    let client = match &config.kratos.client {
        Some(client) => client,
        None => bail!("{request_id}: kratos client not initialized"),
    };

    let mut addr = format!("{}/admin/identities", client.base_path);
    addr = addr + "?credentials_identifier=" + &data.email as &str;
    let response = client.client.get(addr).send().await?;
    response.error_for_status_ref()?;
    let json = response.json::<Vec<Identity>>().await?;
    let identity = match json.get(0) {
        Some(identity) => identity.to_owned(),
        None => bail!("{request_id}: no identity found for {}", data.email),
    };
    debug!("{:?}", identity);
    Ok(identity)
}

async fn send_to_iam(config: Arc<SiriusConfig>, data: Data, request_id: String) -> Result<()> {
    let mut client = config
        .iam
        .client
        .clone()
        .ok_or_else(|| anyhow!("{request_id}: Iam client not initialized!"))?;
    let identity = get_identity_by_mail(&config, &data, &request_id).await?;
    let role = "\"".to_owned() + &data.role as &str + "\"";
    let request = Request::new(Input {
        id: identity.id,
        perm_type: data.ressource_type.clone(),
        resource: data.id.clone(),
        role,
    });
    client.replace_permission(request).await?;
    Ok(())
}

///send a call to iam to update an identity metadata
pub async fn update_controller(
    config: SiriusConfig,
    payload: Vec<Data>,
    request_id: &str,
    identity: Identity,
) -> Result<()> {
    let mut handles = JoinSet::new();
    let config = Arc::new(config);
    for input in payload.iter() {
        if !validate_roles(&config, &identity, &input.id, request_id, "api/iam").await? {
            Err(anyhow!("Invalid role!"))?;
        }
        handles.spawn(send_to_iam(
            config.clone(),
            input.to_owned(),
            request_id.to_owned(),
        ));
    }
    while let Some(future) = handles.join_next().await {
        future??;
    }
    Ok(())
}

async fn get_scope_project(
    client: Arc<Configuration>,
    uuid: String,
    request_id: String,
) -> Result<BTreeSet<String>> {
    let mut projects = BTreeSet::new();
    let identity = get_identity(&client, &uuid, None).await?;
    match identity.metadata_admin {
        Some(metadata) => {
            if let Some(project) = metadata.get("project") {
                let project = project
                    .as_array()
                    .ok_or_else(|| anyhow!("{request_id}: This should be an array!"))?;
                for id in project {
                    let id = id
                        .as_str()
                        .ok_or_else(|| anyhow!("{request_id}: Not a string!"))?;
                    projects.insert(id.to_owned());
                }
            }
        }
        None => {
            error!("{request_id}: no metadata in this user!");
            bail!("{request_id}: no metadata in this user!")
        }
    };
    Ok(projects)
}

pub async fn list_controller(
    config: SiriusConfig,
    request_id: &str,
    identity: Identity,
) -> Result<BTreeSet<String>> {
    let client = match &config.kratos.client {
        Some(client) => client,
        None => bail!("{request_id}: kratos client not initialized"),
    };
    let client = Arc::new(client.to_owned());
    let mut projects = BTreeSet::new();
    let metadata = match identity.metadata_admin {
        Some(metadata) => metadata,
        None => {
            error!("{request_id}: no metadata in this user!");
            bail!("{request_id}: no metadata in this user!")
        }
    };
    if let Some(project) = metadata.get("project") {
        let project = project
            .as_object()
            .ok_or_else(|| anyhow!("{request_id}: This should be a map!"))?;
        for key in project.keys() {
            projects.insert(key.to_owned());
        }
    }
    if let Some(scope) = metadata.get("scope") {
        let scopes = scope.as_object().expect("this should be a map!");
        if !scopes.is_empty() {
            let mut handles = JoinSet::new();
            for (uuid, _) in scopes.iter() {
                handles.spawn(get_scope_project(
                    client.clone(),
                    uuid.to_owned(),
                    request_id.to_owned(),
                ));
            }
            while let Some(future) = handles.join_next().await {
                let mut scopes_proj = future??;
                projects.append(&mut scopes_proj);
            }
        }
    }
    Ok(projects)
}

#[cfg(test)]
pub mod test_controler {
    use mockito::Server as MockServer;

    use super::*;

    use crate::{
        router::Data,
        utils::test::{configure, IDENTITY},
    };

    #[tokio::test]
    async fn test_get_identity_by_mail() {
        let data = Data {
            email: "lol.lol@lol.io".to_owned(),
            ressource_type: "test".to_owned(),
            id: "222".to_owned(),
            role: "admin".to_owned(),
        };
        let uuid = "1";
        let mut kratos_server = MockServer::new_async().await;
        let config = configure(Some(&kratos_server), None, None).await;
        let body = "[".to_owned() + IDENTITY + "]";
        let mock_kratos = kratos_server
            .mock(
                "GET",
                "/admin/identities?credentials_identifier=lol.lol@lol.io",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;
        get_identity_by_mail(&config, &data, uuid).await.unwrap();
        mock_kratos.assert_async().await;
    }

    #[tokio::test]
    async fn test_send_to_iam() {
        let data = Data {
            email: "lol.lol@lol.io".to_owned(),
            ressource_type: "test".to_owned(),
            id: "222".to_owned(),
            role: "admin".to_owned(),
        };
        let uuid = "1".to_owned();
        let mut server = MockServer::new_async().await;
        let config = configure(Some(&server), None, None).await;
        let config = Arc::new(config);
        let body = "[".to_owned() + IDENTITY + "]";
        let mock = server
            .mock(
                "GET",
                "/admin/identities?credentials_identifier=lol.lol@lol.io",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;
        send_to_iam(config, data, uuid).await.unwrap();
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_update_controler_simple() {
        let data = Data {
            email: "lol.lol@lol.io".to_owned(),
            ressource_type: "test".to_owned(),
            id: "222".to_owned(),
            role: "admin".to_owned(),
        };
        let uuid = "1";
        let mut kratos_server = MockServer::new_async().await;
        let mut opa_server = MockServer::new_async().await;
        let config = configure(Some(&kratos_server), Some(&opa_server), None).await;
        let body = "[".to_owned() + IDENTITY + "]";
        let kratos_mock = kratos_server
            .mock(
                "get",
                "/admin/identities?credentials_identifier=lol.lol@lol.io",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;
        let opa_mock = opa_server
            .mock("post", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"true"#)
            .create_async()
            .await;
        let identity = serde_json::from_str(IDENTITY).unwrap();
        update_controller(config, vec![data], uuid, identity)
            .await
            .unwrap();
        kratos_mock.assert_async().await;
        opa_mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_update_controler_multiple() {
        let data = Data {
            email: "lol.lol@lol.io".to_owned(),
            ressource_type: "test".to_owned(),
            id: "222".to_owned(),
            role: "admin".to_owned(),
        };
        let uuid = "1";
        let mut kratos_server = MockServer::new_async().await;
        let mut opa_server = MockServer::new_async().await;
        let config = configure(Some(&kratos_server), Some(&opa_server), None).await;
        let body = "[".to_owned() + IDENTITY + "]";
        let kratos_mock = kratos_server
            .mock(
                "GET",
                "/admin/identities?credentials_identifier=lol.lol@lol.io",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .expect(2)
            .create_async()
            .await;
        let opa_mock = opa_server
            .mock("post", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"true"#)
            .expect(2)
            .create_async()
            .await;

        let identity = serde_json::from_str(IDENTITY).unwrap();
        update_controller(config, vec![data.clone(), data], uuid, identity)
            .await
            .unwrap();
        kratos_mock.assert_async().await;
        opa_mock.assert_async().await;
    }
}
