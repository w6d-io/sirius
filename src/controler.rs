use anyhow::{anyhow, bail, Result};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use ory_kratos_client::models::Identity;
use tonic::{transport::Channel, Request};
use tracing::{error, debug};

use crate::{
    config::SiriusConfig,
    permission::{iam_client::IamClient, Input as TonicInput},
    router::Input,
};

///get an identities form kratos by mail
async fn get_identity_by_mail(
    config: &SiriusConfig,
    input: &Input,
    uuid: &str,
) -> Result<Identity> {
    let client = match &config.kratos.client {
        Some(client) => client,
        None => bail!("{uuid}: kratos client not initialized"),
    };

    let mut addr = format!("{}/admin/identities", client.base_path);
    addr = addr + "?credentials_identifie=" + &input.email as &str;
    let response = client.client.get(addr).send().await?;
    response.error_for_status_ref()?;
    let json = response.json::<Vec<Identity>>().await?;
    let identity = match json.get(0) {
        Some(identity) => identity.to_owned(),
        None => bail!("{uuid}: no identity found for {}", input.email),
    };
    debug!("{:?}", identity);
    Ok(identity)
}

async fn send_to_iam(
    mut client: IamClient<Channel>,
    config: &SiriusConfig,
    input: &Input,
    uuid: &str,
) -> Result<()> {
    let identity = get_identity_by_mail(config, input, uuid).await?;
    let role = "\"".to_owned() + &input.role as &str + "\"";
    let request = Request::new(TonicInput {
        id: identity.id,
        perm_type: input.ressource_type.clone(),
        resource: input.id.clone(),
        role,
    });
    client.replace_permission(request).await?;
    Ok(())
}

///send a call to iam to update an identity metadata
pub async fn update_controler(
    config: &SiriusConfig,
    payload: Vec<Input>,
    uuid: &str,
) -> Result<()> {
    let client = config
        .iam
        .client
        .clone()
        .ok_or_else(|| anyhow!("{uuid}: Iam client not initialized!"))?;
    let mut futures = FuturesUnordered::new();
    for input in payload.iter() {
        let client = client.clone();
        futures.push(send_to_iam(client, config, input, uuid));
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
mod test_controler{
    use super::*;

    use tower::service_fn;
    use tonic::{transport::{Server, Endpoint, Uri}, async_trait, Response, Status, Request};

    use crate::permission::{iam_server::{IamServer, Iam}, Reply, Input};


    #[derive(Default)]
    pub struct MyIam {
    }

    #[async_trait]
    impl Iam for MyIam {    ///grpc route to replace an identity field
        async fn add_permission(
            &self,
            req: Request<Input>,
            ) -> Result<Response<Reply>, Status> {
            println!("replace: Got a request: {:?}", req); 
            Ok(Response::new(Reply {}))
        }
        async fn remove_permission(
            &self,
            req: Request<Input>,
            ) -> Result<Response<Reply>, Status> {
            println!("replace: Got a request: {:?}", req); 
            Ok(Response::new(Reply {}))
        }
        async fn replace_permission(
            &self,
            req: Request<Input>,
            ) -> Result<Response<Reply>, Status> {
            println!("replace: Got a request: {:?}", req);
            Ok(Response::new(Reply {}))
        }
    }


    pub async fn mock_grpc_server() -> Result<IamClient<Channel>>{
        let (client, server) = tokio::io::duplex(1024);

        let greeter = MyIam::default();

        tokio::spawn(async move {
            Server::builder()
                .add_service(IamServer::new(greeter))
                .serve_with_incoming(futures::stream::iter(vec![Ok::<_, std::io::Error>(server)]))
                .await
        });
        let mut client = Some(client);
        let channel = Endpoint::try_from("http://[::]:50051")?
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
        .await?;
        Ok(IamClient::new(channel))
    }


}


