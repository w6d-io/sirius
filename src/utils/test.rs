use mockito::Server as MockServer;
use tonic::{
    async_trait,
    transport::{Channel, Endpoint, Server, Uri},
    Request, Response, Status,
};
use tower::service_fn;

use rs_utils::kratos::Kratos;

use crate::{
    config::{Iam, Ports, Service, SiriusConfig},
    permission::{
        iam_client::IamClient,
        iam_server::{Iam as IamTrait, IamServer},
        Input, Reply,
    },
};

pub static IDENTITY_ORG: &str = r#"
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
        "metadata_admin":null,
        "metadata_public":{
            "project":{},
            "group":{
                "7113206d-afc0-41ad-bbca-b1e8113beb82": "default"
            }
        },
        "created_at":"2023-03-17T14:48:52.000392Z",
        "updated_at":"2023-03-17T14:48:52.000392Z"
    }"#;

pub static IDENTITY_GROUP: &str = r#"
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
        "metadata_admin":null,
        "metadata_public":{
            "project":[122, 334, 456]
        },
        "created_at":"2023-03-17T14:48:52.000392Z",
        "updated_at":"2023-03-17T14:48:52.000392Z"
    }"#;

pub static IDENTITY_USER: &str = r#"
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
        "metadata_admin":null,
        "metadata_public":{
            "project":{},
            "group":{
                "7113206d-afc0-41ad-bbca-b1e8113beb82": {
                    "name" : "awesome",
                    "project" : [122, 334, 456]
                }
            }
        },
        "created_at":"2023-03-17T14:48:52.000392Z",
        "updated_at":"2023-03-17T14:48:52.000392Z"
    }"#;

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

pub async fn configure(
    kratos_serv: Option<&MockServer>,
    opa: Option<&MockServer>,
    iam: Option<&MockServer>,
) -> SiriusConfig {
    let mut kratos = Kratos {
        addr: match kratos_serv {
            Some(serv) => serv.url(),
            None => "http://0.0.0.0:9000".to_owned(),
        },
        client: None,
    };
    kratos.update();
    let mut conf = SiriusConfig::default();
    conf.opa.mode = "public".to_owned();
    conf.opa.addr = match opa {
        Some(opa) => opa.url(),
        None => "http://0.0.0.0:8000".to_owned(),
    };
    conf.service = Service {
        addr: "0.0.0.0".to_owned(),
        ports: Ports {
            main: "8080".to_owned(),
            health: "8181".to_owned(),
        },
    };
    let iam_url = match iam {
        Some(iam) => iam.host_with_port(),
        None => "0.0.0.0:8383".to_owned(),
    };

    let (addr, port) = iam_url.split_once(':').unwrap();
    conf.iam = Iam {
        service: Service {
            addr: addr.to_owned(),
            ports: Ports {
                main: "8282".to_owned(),
                health: port.to_owned(),
            },
        },
        client: Some(mock_grpc_server().await),
    };
    conf.kratos = kratos;
    conf
}
