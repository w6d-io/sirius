use anyhow::{anyhow, bail, Result};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use ory_kratos_client::models::Identity;
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
    addr = addr + "?credentials_identifie=" + &data.email as &str;
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

async fn send_to_iam(config: &SiriusConfig, data: &Data, request_id: &str) -> Result<()> {
    let mut client = config
        .iam
        .client
        .clone()
        .ok_or_else(|| anyhow!("{request_id}: Iam client not initialized!"))?;
    let identity = get_identity_by_mail(config, data, request_id).await?;
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
pub async fn update_controler(
    config: &SiriusConfig,
    payload: Vec<Data>,
    request_id: &str,
    identity: Identity,
) -> Result<()> {
    let mut futures = FuturesUnordered::new();
    for input in payload.iter() {
        if !validate_roles(config, &identity, &input.id, request_id, "api/iam").await? {
            Err(anyhow!("Invalid role!"))?;
        }
        futures.push(send_to_iam(config, input, request_id));
    }
    while let Some(res) = futures.next().await {
        if let Err(e) = res {
            error!("{request_id}: one or more request failed: {e}");
            bail!("{request_id}: one or more request failed: {e}")
        }
    }
    Ok(())
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
                "/admin/identities?credentials_identifie=lol.lol@lol.io",
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
        let uuid = "1";
        let mut server = MockServer::new_async().await;
        let config = configure(Some(&server), None, None).await;
        let body = "[".to_owned() + IDENTITY + "]";
        let mock = server
            .mock(
                "GET",
                "/admin/identities?credentials_identifie=lol.lol@lol.io",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;
        send_to_iam(&config, &data, uuid).await.unwrap();
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
                "/admin/identities?credentials_identifie=lol.lol@lol.io",
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
        update_controler(&config, vec![data], uuid, identity)
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
                "/admin/identities?credentials_identifie=lol.lol@lol.io",
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
        update_controler(&config, vec![data.clone(), data], uuid, identity)
            .await
            .unwrap();
        kratos_mock.assert_async().await;
        opa_mock.assert_async().await;
    }
}
