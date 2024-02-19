use std::sync::Arc;

use anyhow::{anyhow, bail, Ok, Result};
use ory_kratos_client::{
    apis::{configuration::Configuration, identity_api::get_identity},
    models::Identity,
};
use tokio::task::JoinSet;
use tonic::Request;
use tracing::{debug, info};

#[cfg(feature = "opa")]
use crate::utils::opa::validate_roles;
use crate::{
    config::SiriusConfig,
    permission::{Input, Mode},
    router::{Data, IDType},
};

///get an identities form kratos by mail
async fn get_identity_by_mail(client: &Configuration, id: &str) -> Result<Identity> {
    let mut addr = format!("{}/admin/identities", client.base_path);
    addr = addr + "?credentials_identifier=" + id;
    let response = client.client.get(addr).send().await?;
    response.error_for_status_ref()?;
    let json = response.json::<Vec<Identity>>().await?;
    let identity = match json.first() {
        Some(identity) => identity.to_owned(),
        None => bail!("no identity found for {}", id),
    };
    debug!("{:?}", identity);
    Ok(identity)
}

async fn get_kratos_identity(config: &SiriusConfig, id: &IDType) -> Result<Identity> {
    let client = match &config.kratos.client {
        Some(client) => client,
        None => bail!("kratos client not initialized"),
    };
    let identity = match id {
        IDType::Email(id) => get_identity_by_mail(client, id.as_str()).await?,
        IDType::ID(ref id) => get_identity(client, &id.to_string(), None).await?,
    };
    Ok(identity)
}

async fn send_to_iam(identity: Arc<Identity>, config: Arc<SiriusConfig>, data: Data) -> Result<()> {
    let mut client = config
        .iam
        .client
        .clone()
        .ok_or_else(|| anyhow!(" Iam client not initialized!"))?;
    let value = data.value.to_string();
    info!("{value}");

    let mut input = Input {
        id: identity.id.clone(),
        perm_type: data.ressource_type.clone(),
        resource: data.ressource_id,
        value,
        ..Default::default()
    };
    let mode = match &config.opa.mode as &str {
        "admin" => Mode::Admin,
        "public" => Mode::Public,
        "trait" => Mode::Trait,
        _ => bail!("Invalid mode! please put a valid mode (admin, public or trait) in the config"),
    };
    input.set_mode(mode);
    let request = Request::new(input);
    client.add_permission(request).await?;
    Ok(())
}

///send a call to iam to update an identity metadata
pub async fn update_controller(
    config: Arc<SiriusConfig>,
    payload: Vec<Data>,
    _identity: Identity,
    endpoint: &str,
) -> Result<Identity> {
    let mut handles = JoinSet::new();
    let mut object_identity: Option<Arc<Identity>> = None;
    let _uri = "api/iam/".to_owned() + endpoint;
    for data in payload.iter() {
        #[cfg(feature = "opa")]
        if !validate_roles(&config, &_identity, &data.ressource_id, &_uri).await? {
            Err(anyhow!("Invalid role!"))?;
        }
        println!("role validated!");
        if let Some(ref ident) = object_identity {
            match data.id {
                IDType::ID(id) if id.to_string() != ident.id => {
                    object_identity = Some(Arc::new(get_kratos_identity(&config, &data.id).await?))
                }
                IDType::Email(ref id) => match &ident.traits {
                    Some(traits) => match traits.get("email") {
                        Some(email) if email == id.as_str() => (),
                        _ => {
                            object_identity =
                                Some(Arc::new(get_kratos_identity(&config, &data.id).await?))
                        }
                    },
                    None => {
                        object_identity =
                            Some(Arc::new(get_kratos_identity(&config, &data.id).await?))
                    }
                },
                _ => (),
            }
        } else {
            object_identity = Some(Arc::new(get_kratos_identity(&config, &data.id).await?));
        }
        info!("kratos identity obtained!");
        match object_identity {
            Some(ref ident) => {
                handles.spawn(send_to_iam(ident.clone(), config.clone(), data.to_owned()));
            }
            None => bail!("the identity is not initialized this should not be happening!"),
        }
    }
    while let Some(future) = handles.join_next().await {
        future??;
    }
    let ret =
        Arc::try_unwrap(object_identity.unwrap()).map_err(|_| anyhow!("failed to uwrap arc"))?;
    Ok(ret)
}

#[cfg(test)]
pub mod test_controler {
    use super::*;
    use mockito::Server as MockServer;
    use serde_email::Email;
    use serde_json::Value;

    use crate::{
        router::Data,
        utils::test::{configure, IDENTITY_USER},
    };

    #[tokio::test]
    async fn test_get_kratos_identity_email() {
        let id = IDType::Email(Email::from_str("lol.lol@lol.io").unwrap());
        let mut kratos_server = MockServer::new_async().await;
        let config = configure(Some(&kratos_server), None, None).await;
        let body = "[".to_owned() + IDENTITY_USER + "]";
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
        get_kratos_identity(&config, &id).await.unwrap();
        mock_kratos.assert_async().await;
    }

    #[tokio::test]
    async fn test_send_to_iam_email() {
        let data = Data {
            id: IDType::Email(Email::from_str("lol.lol@lol.io").unwrap()),
            ressource_type: "test".to_owned(),
            ressource_id: "222".to_owned(),
            value: Value::Array(vec![Value::String("admin".to_owned())]),
        };
        let identity = Arc::new(serde_json::from_str(IDENTITY_USER).unwrap());
        let config = configure(None, None, None).await;
        let config = Arc::new(config);
        send_to_iam(identity, config, data).await.unwrap();
    }

    #[tokio::test]
    async fn test_update_controler_simple() {
        let data = Data {
            id: IDType::Email(Email::from_str("lol.lol@lol.io").unwrap()),
            ressource_type: "test".to_owned(),
            ressource_id: "222".to_owned(),
            value: Value::Array(vec![Value::String("admin".to_owned())]),
        };
        let mut kratos_server = MockServer::new_async().await;
        let opa_server = MockServer::new_async().await;
        let config = configure(Some(&kratos_server), Some(&opa_server), None).await;
        let body = "[".to_owned() + IDENTITY_USER + "]";
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
        /* let opa_mock = opa_server
        .mock("post", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"true"#)
        .create_async()
        .await; */
        let identity = serde_json::from_str(IDENTITY_USER).unwrap();
        update_controller(Arc::new(config), vec![data], identity, "project")
            .await
            .unwrap();
        kratos_mock.assert_async().await;
        // opa_mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_update_controler_multiple() {
        let data = Data {
            id: IDType::Email(Email::from_str("lol.lol@lol.io").unwrap()),
            ressource_type: "test".to_owned(),
            ressource_id: "222".to_owned(),
            value: Value::String("admin".to_owned()),
        };
        let mut kratos_server = MockServer::new_async().await;
        let opa_server = MockServer::new_async().await;
        let config = configure(Some(&kratos_server), Some(&opa_server), None).await;
        let body = "[".to_owned() + IDENTITY_USER + "]";
        let kratos_mock = kratos_server
            .mock(
                "GET",
                "/admin/identities?credentials_identifier=lol.lol@lol.io",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;
        /* let opa_mock = opa_server
        .mock("post", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"true"#)
        .expect(2)
        .create_async()
        .await; */

        let identity = serde_json::from_str(IDENTITY_USER).unwrap();
        update_controller(
            Arc::new(config),
            vec![data.clone(), data],
            identity,
            "project",
        )
        .await
        .unwrap();
        kratos_mock.assert_async().await;
        // opa_mock.assert_async().await;
    }
}
