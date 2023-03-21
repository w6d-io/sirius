use anyhow::{anyhow, bail, Result};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use ory_kratos_client::models::Identity;
use tonic::Request;
use tracing::{debug, error};

use crate::{config::SiriusConfig, permission::Input, router::Data};

///get an identities form kratos by mail
async fn get_identity_by_mail(config: &SiriusConfig, data: &Data, uuid: &str) -> Result<Identity> {
    let client = match &config.kratos.client {
        Some(client) => client,
        None => bail!("{uuid}: kratos client not initialized"),
    };

    let mut addr = format!("{}/admin/identities", client.base_path);
    addr = addr + "?credentials_identifie=" + &data.email as &str;
    let response = client.client.get(addr).send().await?;
    response.error_for_status_ref()?;
    let json = response.json::<Vec<Identity>>().await?;
    let identity = match json.get(0) {
        Some(identity) => identity.to_owned(),
        None => bail!("{uuid}: no identity found for {}", data.email),
    };
    debug!("{:?}", identity);
    Ok(identity)
}

async fn send_to_iam(config: &SiriusConfig, data: &Data, uuid: &str) -> Result<()> {
    let mut client = config
        .iam
        .client
        .clone()
        .ok_or_else(|| anyhow!("{uuid}: Iam client not initialized!"))?;
    let identity = get_identity_by_mail(config, data, uuid).await?;
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
pub async fn update_controler(config: &SiriusConfig, payload: Vec<Data>, uuid: &str) -> Result<()> {
    let mut futures = FuturesUnordered::new();
    for input in payload.iter() {
        futures.push(send_to_iam(config, input, uuid));
    }
    while let Some(res) = futures.next().await {
        if let Err(e) = res {
            error!("{uuid}: one or more request failed: {e}");
            bail!("{uuid}: one or more request failed: {e}")
        }
    }
    Ok(())
}

#[cfg(test)]
mod test_controler {
    use mockito::Server as MockServer;
    use tonic::{
        async_trait,
        transport::{Channel, Endpoint, Server, Uri},
        Request, Response, Status,
    };
    use tower::service_fn;

    use rs_utils::kratos::Kratos;

    use super::*;
    use crate::{
        config::{Iam, Ports, Service},
        permission::{
            iam_client::IamClient,
            iam_server::{Iam as IamTrait, IamServer},
            Input, Reply,
        },
        router::Data,
    };

    static PAYLOAD: &str = r#"
        [
            {
                "id":"af25f904-5319-4011-95a4-343365d64811",
                "schema_id":"default",
                "schema_url":"http://127.0.0.1:4433/schemas/ZGVmYXVsdA",
                "state":"active",
                "state_changed_at":"2023-03-17T14:48:51.999240392Z",
                "traits":{
                    "email":"lol.lol@lol.io",
                    "name":{
                        "first":"lol",
                        "last":"lol"
                    }
                },
                "verifiable_addresses":[
                    {
                        "id":"9a93298f-50c5-4ee0-a9b9-95632da77cd7",
                        "value":"lol.lol@lol.io",
                        "verified":false,
                        "via":"email",
                        "status":"sent",
                        "created_at":"2023-03-17T14:48:52.000813Z",
                        "updated_at":"2023-03-17T14:48:52.000813Z"
                    }
                ],
                "recovery_addresses":[
                    {
                        "id":"2a675f07-c733-4dce-a280-1fb9054d4a74",
                        "value":"lol.lol@lol.io",
                        "via":"email",
                        "created_at":"2023-03-17T14:48:52.001108Z",
                        "updated_at":"2023-03-17T14:48:52.001108Z"
                    }
                ],
                "metadata_public":null,
                "metadata_admin":{
                "project":{},
                "scope":{
                    "222":"lol"
                    }
                },
                "created_at":"2023-03-17T14:48:52.000392Z",
                "updated_at":"2023-03-17T14:48:52.000392Z"
            }
        ]"#;

    #[derive(Default)]
    pub struct MyIam {}

    #[async_trait]
    impl IamTrait for MyIam {
        ///grpc route to replace an identity field
        async fn add_permission(&self, req: Request<Input>) -> Result<Response<Reply>, Status> {
            println!("replace: Got a request: {:?}", req);
            Ok(Response::new(Reply {}))
        }
        async fn remove_permission(&self, req: Request<Input>) -> Result<Response<Reply>, Status> {
            println!("replace: Got a request: {:?}", req);
            Ok(Response::new(Reply {}))
        }
        async fn replace_permission(&self, req: Request<Input>) -> Result<Response<Reply>, Status> {
            println!("replace: Got a request: {:?}", req);
            Ok(Response::new(Reply {}))
        }
    }

    pub async fn mock_grpc_server() -> IamClient<Channel> {
        let (client, server) = tokio::io::duplex(1024);

        let greeter = MyIam::default();

        tokio::spawn(async move {
            Server::builder()
                .add_service(IamServer::new(greeter))
                .serve_with_incoming(futures::stream::iter(vec![Ok::<_, std::io::Error>(server)]))
                .await
        });
        let mut client = Some(client);
        let channel = Endpoint::try_from("http://[::]:50051")
            .unwrap()
            .connect_with_connector(service_fn(move |_: Uri| {
                let client = client.take();

                async move {
                    if let Some(client) = client {
                        Ok(client)
                    } else {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Client already taken",
                        ))
                    }
                }
            }))
            .await
            .unwrap();
        IamClient::new(channel)
    }

    async fn configure(kratos_serv: &MockServer) -> SiriusConfig {
        let mut kratos = Kratos {
            addr: kratos_serv.url(),
            client: None,
        };
        kratos.update();
        let mut conf = SiriusConfig::default();
        conf.service = Service {
            addr: "0.0.0.0".to_owned(),
            ports: Ports {
                main: "8080".to_owned(),
                health: "8181".to_owned(),
            },
        };
        conf.iam = Iam {
            service: Service {
                addr: "0.0.0.0".to_owned(),
                ports: Ports {
                    main: "8282".to_owned(),
                    health: "8383".to_owned(),
                },
            },
            client: Some(mock_grpc_server().await),
        };
        conf.kratos = kratos;
        conf
    }

    #[tokio::test]
    async fn test_get_identity_by_mail() {
        let data = Data {
            email: "lol.lol@lol.io".to_owned(),
            ressource_type: "test".to_owned(),
            id: "222".to_owned(),
            role: "admin".to_owned(),
        };
        let uuid = "1";
        let mut server = MockServer::new_async().await;
        let config = configure(&server).await;
        let mock = server
            .mock(
                "GET",
                "/admin/identities?credentials_identifie=lol.lol@lol.io",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(PAYLOAD)
            .create_async()
            .await;
        get_identity_by_mail(&config, &data, uuid).await.unwrap();
        mock.assert_async().await;
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
        let config = configure(&server).await;
        let mock = server
            .mock(
                "GET",
                "/admin/identities?credentials_identifie=lol.lol@lol.io",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(PAYLOAD)
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
        let mut server = MockServer::new_async().await;
        let config = configure(&server).await;
        let mock = server
            .mock(
                "GET",
                "/admin/identities?credentials_identifie=lol.lol@lol.io",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(PAYLOAD)
            .create_async()
            .await;
        update_controler(&config, vec![data], uuid).await.unwrap();
        mock.assert_async().await;
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
        let mut server = MockServer::new_async().await;
        let config = configure(&server).await;
        let mock = server
            .mock(
                "GET",
                "/admin/identities?credentials_identifie=lol.lol@lol.io",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(PAYLOAD)
            .expect(2)
            .create_async()
            .await;
        update_controler(&config, vec![data.clone(), data], uuid)
            .await
            .unwrap();
        mock.assert_async().await;
    }
}
