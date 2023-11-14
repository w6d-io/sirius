use std::{sync::Arc, time::Duration};

use anyhow::{anyhow, bail, Result};
use kafka::{
    producer::{future_producer::DefaultFutureContext, FutureProducer},
    KafkaMessage, KafkaProducer,
};
use ory_kratos_client::models::Identity;
use serde_json::{json, Value};
use tonic::Request;
use tracing::{error, log::info};

use crate::{
    config::SiriusConfig,
    permission::{Input, Mode},
};

async fn extract_sync_id(
    identity: &mut Identity,
    request_id: &str,
    sync_type: &str,
    config: &Arc<SiriusConfig>,
) -> Result<Vec<String>> {
    let mut meta = match &config.mode as &str {
        "admin" => &mut identity.metadata_admin,
        "public" => &mut identity.metadata_public,
        "trait" => &mut identity.traits,
        _ => bail!("Invalid mode! please put a valid mode (admin, public or trait) in the config"),
    };

    let metadata = match &mut meta {
        Some(ref mut metadata) => metadata,
        None => {
            error!("{request_id}: no metadata in this group!");
            bail!("{request_id}: no metadata in this group!")
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

#[derive(Debug)]
pub enum SyncMode {
    User(Vec<(String, Value)>),
    Project(Vec<String>),
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

pub async fn sync_groups(
    config: Arc<SiriusConfig>,
    identity: &Identity,
    request_id: &str,
    users: &[(String, Value)],
) -> Result<()> {
    info!("recuparating groups from identity");
    let meta = match &config.mode as &str {
        "admin" => &identity.metadata_admin,
        "public" => &identity.metadata_public,
        "trait" => &identity.traits,
        _ => bail!("Invalid mode! please put a valid mode (admin, public or trait) in the config"),
    };
    let groups = match meta {
        Some(ref meta) => match meta.get("group") {
            Some(proj) => proj
                .as_object()
                .ok_or_else(|| anyhow!("{request_id}: not an object!"))?,
            None => {
                bail!("{request_id}: no groups in metadata!")
            }
        },
        None => {
            bail!("{request_id}: this organisation as no metadata!")
        }
    };
    let mut def_group_id = String::new();
    info!("recuparating default group");
    for (id, data) in groups {
        println!("data: {}", data);
        let name = data
            .as_str()
            .ok_or_else(|| anyhow!("{request_id}: name not a string!"))?;
        if name == "default" {
            def_group_id = id.to_owned();
        }
    }
    info!("sending payload to iam!");
    for (user, role) in users {
        info!("patching user: {user}.");
        send_to_iam(&config, &def_group_id, user, role, request_id, "user").await?;
    }
    Ok(())
}

pub async fn sync_user(
    config: Arc<SiriusConfig>,
    identity: Identity,
    request_id: String,
    mode: SyncMode,
) {
    let kafka_error = get_producer(&config, "error", &request_id);
    let kafka_notif = get_producer(&config, "notif", &request_id);
    match sync(config, identity, &request_id, mode).await {
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
            info!("data synced successfully!");
        }
        Err(e) => {
            error!("an error has occurred when syncing data: {e}");
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
    id: &str,
    ressource_id: &str,
    json: &serde_json::Value,
    request_id: &str,
    perm_type: &str,
) -> Result<()> {
    let mut iam_client = config
        .iam
        .client
        .clone()
        .ok_or_else(|| anyhow!("{request_id}: Iam client not initialized!"))?;
    let mut input = Input {
        id: id.to_owned(),
        perm_type: perm_type.to_owned(),
        resource: ressource_id.to_owned(),
        value: serde_json::to_string(&json).unwrap(),
        ..Default::default()
    };
    let mode = match &config.mode as &str {
        "admin" => Mode::Admin,
        "public" => Mode::Public,
        "trait" => Mode::Trait,
        _ => bail!("Invalid mode! please put a valid mode (admin, public or trait) in the config"),
    };
    input.set_mode(mode);
    let request = Request::new(input);
    iam_client.replace_permission(request).await?;
    Ok(())
}

pub async fn sync(
    config: Arc<SiriusConfig>,
    mut identity: Identity,
    request_id: &str,
    mode: SyncMode,
) -> Result<()> {
    let id = identity.id.clone();
    let meta = match &config.mode as &str {
        "admin" => &mut identity.metadata_admin,
        "public" => &mut identity.metadata_public,
        "trait" => &mut identity.traits,
        _ => bail!("Invalid mode! please put a valid mode (admin, public or trait) in the config"),
    };

    let mut projects = match meta {
        Some(ref mut meta) => match meta.get_mut("project") {
            Some(proj) => {
                let old_projects = proj
                    .as_object()
                    .ok_or_else(|| anyhow!("{request_id}: not an object"))?;
                old_projects
                    .keys()
                    .map(|e| e.as_str().to_owned())
                    .collect::<Vec<String>>()
            }
            None => Vec::new(),
        },
        None => {
            bail!("{request_id}: this group as no metadata!");
        }
    };

    info!("old project: {projects:?}");
    info!("sync mode: {mode:?}");
    match mode {
        SyncMode::Project(data) => {
            for new_project in data {
                if !projects.contains(&new_project) {
                    projects.push(new_project);
                }
            }
            let json = json!({ "project": projects });
            info!("new project list: {json}");
            let users = extract_sync_id(&mut identity, request_id, "user", &config).await?;
            for user in users {
                send_to_iam(&config, &user, &id, &json, request_id, "group").await?;
            }
        }
        SyncMode::User(data) => {
            for (user, role) in data {
                let name = match identity.traits {
                    Some(ref mut traits) => traits
                        .get_mut("name")
                        .ok_or_else(|| anyhow!("{request_id}: this group as no name!"))?
                        .take(),
                    None => bail!("{request_id}: this group as no trait!"),
                };
                let json = json!({
                    "name": name,
                    "project": projects,
                    "role": role
                });
                send_to_iam(&config, &user, &id, &json, request_id, "group").await?;
            }
        }
    };
    Ok(())
}

#[cfg(test)]
mod test_sync {
    use serde_json::Value;
    use std::sync::Arc;
    use uuid::Uuid;

    use crate::utils::test::{configure, IDENTITY_GROUP, IDENTITY_ORG};

    use super::*;

    #[tokio::test]
    async fn test_send_to_iam() {
        let json = Value::Array(vec![Value::String("admin".to_owned())]);
        let user = Uuid::new_v4().to_string();
        let id = Uuid::new_v4().to_string();
        let uuid = "1";
        let config = configure(None, None, None).await;
        let config = Arc::new(config);
        send_to_iam(&config, &user, &id, &json, uuid, "groups")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_sync_simple() {
        let identity = serde_json::from_str(IDENTITY_GROUP).unwrap();
        let uuid = "1";
        let mode = SyncMode::Project(vec!["test".to_owned(), "test".to_owned()]);
        let config = configure(None, None, None).await;
        let config = Arc::new(config);
        sync(config, identity, uuid, mode).await.unwrap();
    }

    #[tokio::test]
    async fn test_sync_groups_simple() {
        let identity = serde_json::from_str(IDENTITY_ORG).unwrap();
        let uuid = "1";
        let user = &[("test".to_owned(), Value::Null)];
        let config = configure(None, None, None).await;
        let config = Arc::new(config);
        sync_groups(config, &identity, uuid, user).await.unwrap();
    }
}
