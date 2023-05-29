use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{anyhow, bail, Result};
use kafka::{
    producer::{future_producer::DefaultFutureContext, FutureProducer},
    KafkaMessage, KafkaProducer,
};
use ory_kratos_client::models::Identity;
use serde_json::{json, Value};
use tonic::Request;
use tracing::{error, log::warn};

use crate::{
    config::SiriusConfig,
    permission::{Input, Mode},
};

async fn extract_sync_id(
    identity: &mut Identity,
    request_id: &str,
    sync_type: &str,
) -> Result<Vec<String>> {
    let metadata = match &mut identity.metadata_admin {
        Some(ref mut metadata) => metadata,
        None => {
            error!("{request_id}: no metadata in this scope!");
            bail!("{request_id}: no metadata in this scope!")
        }
    };
    let mut ret = Vec::new();
    if let Some(val) = metadata.get_mut(sync_type) {
        if let Some(val) = val.take().as_object() {
            for k in val.keys() {
                ret.push(k.clone());
            }
        }
    };
    Ok(ret)
}

pub enum SyncMode {
    User(HashMap<String, (String, Value)>),
    Project(HashMap<String, String>),
}

fn get_producer(
    config: &Arc<SiriusConfig>,
    producer: &str,
    request_id: &str,
) -> KafkaProducer<FutureProducer, DefaultFutureContext> {
    match config.kafka.producers.get(producer) {
        Some(producer) => match producer.client.clone() {
            Some(p) => p,
            None => {
                error!("{request_id}: producer not initialized");
                panic!();
            }
        },
        None => {
            error!("{request_id}: no {producer} producer");
            panic!();
        }
    }
}

pub async fn sync_user(
    config: Arc<SiriusConfig>,
    identitys: Vec<Identity>,
    request_id: String,
    mode: SyncMode,
) {
    let kafka_error = get_producer(&config, "error", &request_id);
    let kafka_notif = get_producer(&config, "notif", &request_id);
    match sync(config, identitys, &request_id, mode).await {
        Ok(_) => {
            let message = KafkaMessage {
                payload: "ok".to_string(),
                key: None,
                headers: None,
            };
            if let Err(e) = kafka_notif
                .produce(message, Some(Duration::from_secs(30)))
                .await
            {
                let message = KafkaMessage {
                    payload: e.to_string(),
                    key: None,
                    headers: None,
                };
                if let Err(e) = kafka_error
                    .produce(message, Some(Duration::from_secs(30)))
                    .await
                {
                    error!("{request_id}: {e}");
                    panic!();
                }
            }
        }
        Err(e) => {
            let message = KafkaMessage {
                payload: e.to_string(),
                key: None,
                headers: None,
            };
            let res = kafka_error
                .produce(message, Some(Duration::from_secs(30)))
                .await;
            let message = KafkaMessage {
                payload: "ko".to_string(),
                key: None,
                headers: None,
            };
            if let Err(e) = kafka_notif
                .produce(message, Some(Duration::from_secs(30)))
                .await
            {
                error!("{request_id}: {e}");
                panic!();
            }
            if let Err(e) = res {
                error!("{request_id}: {e}");
                panic!();
            }
        }
    }
}

async fn send_to_iam(
    config: &Arc<SiriusConfig>,
    user: String,
    id: String,
    json: &serde_json::Value,
    request_id: &str,
) -> Result<()> {
    let mut iam_client = config
        .iam
        .client
        .clone()
        .ok_or_else(|| anyhow!("{request_id}: Iam client not initialized!"))?;
    let mut input = Input {
        id: user,
        perm_type: "scope".to_owned(),
        resource: id,
        value: serde_json::to_string(&json).unwrap(),
        ..Default::default()
    };
    input.set_mode(Mode::Meta);
    let request = Request::new(input);
    iam_client.replace_permission(request).await?;
    Ok(())
}

pub async fn sync(
    config: Arc<SiriusConfig>,
    mut identities: Vec<Identity>,
    request_id: &str,
    mut mode: SyncMode,
) -> Result<()> {
    while let Some(mut identity) = identities.pop() {
        let id = identity.id.clone();
        let mut projects = match identity.metadata_admin {
            Some(ref mut meta) => match meta.get_mut("projects") {
                Some(proj) => proj.take(),
                None => Value::Null,
            },
            None => {
                error!("{request_id}: this scope as no metadata!");
                break;
            }
        };
        match &mut mode {
            SyncMode::Project(data) => match data.remove(&id) {
                Some(new_project) => {
                    projects
                        .as_array_mut()
                        .ok_or_else(|| anyhow!("{request_id}: not an array"))?
                        .push(Value::String(new_project));
                    let users = extract_sync_id(&mut identity, request_id, "user").await?;
                    let json = json!({ "projects": projects });
                    for user in users {
                        send_to_iam(&config, user, id.clone(), &json, request_id).await?;
                    }
                }
                None => {
                    warn!("{request_id}: no corresponding project found!");
                }
            },
            SyncMode::User(data) => match data.remove(&id) {
                Some((user, role)) => {
                    let name = match identity.traits {
                        Some(ref mut traits) => traits
                            .get_mut("name")
                            .ok_or_else(|| anyhow!("{request_id}: this scope as no name!"))?
                            .take(),
                        None => bail!("{request_id}: this scope as no trait!"),
                    };
                    let json = json!({
                        "name": name,
                        "projects": projects,
                        "role": role
                    });
                    send_to_iam(&config, user, id, &json, request_id).await?;
                }
                None => {
                    warn!("{request_id}: no corresponding user found!");
                }
            },
        };
    }
    Ok(())
}

#[cfg(test)]
mod test_sync {
    use serde_json::Value;
    use std::sync::Arc;
    use uuid::Uuid;

    use crate::utils::test::{configure, IDENTITY};

    use super::*;

    #[tokio::test]
    async fn test_send_to_iam() {
        let json = Value::Array(vec![Value::String("admin".to_owned())]);
        let user = Uuid::new_v4().to_string();
        let id = Uuid::new_v4().to_string();
        let uuid = "1";
        let config = configure(None, None, None).await;
        let config = Arc::new(config);
        send_to_iam(&config, user, id, &json, uuid).await.unwrap();
    }

    #[tokio::test]
    async fn test_sync_scopes_simple() {
        let identitys = vec![serde_json::from_str(IDENTITY).unwrap()];
        let uuid = "1";
        let mode = SyncMode::Project(HashMap::from([("test".to_owned(), "test".to_owned())]));
        let config = configure(None, None, None).await;
        let config = Arc::new(config);
        sync(config, identitys, uuid, mode).await.unwrap();
    }
}
