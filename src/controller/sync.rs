use std::{collections::HashMap, ops::Deref, sync::Arc};

use anyhow::{anyhow, bail, Result};
use ory_kratos_client::models::Identity;
use serde_json::{json, Value};
use tonic::Request;
use tracing::{error, log::info};

use crate::{
    config::SiriusConfig,
    permission::{Input, Mode},
    utils::{error::send_error, kafka::send_to_kafka},
};

async fn extract_sync_id(
    identity: &mut Identity,
    sync_type: &str,
    config: &Arc<SiriusConfig>,
) -> Result<HashMap<String, Vec<Value>>> {
    let mut meta = match &config.opa.mode as &str {
        "admin" => &mut identity.metadata_admin,
        "public" => &mut identity.metadata_public,
        "trait" => &mut identity.traits,
        _ => bail!("Invalid mode! please put a valid mode (admin, public or trait) in the config"),
    };

    let metadata = match &mut meta {
        Some(ref mut metadata) => metadata,
        None => {
            error!("No metadata in this group!");
            bail!("No metadata in this group!")
        }
    };
    let mut ret = HashMap::new();
    if let Some(val) = metadata.get(sync_type) {
        if let Some(val) = val.as_object() {
            for (k, v) in val {
                let v = match v.as_array() {
                    Some(value) => value.to_owned(),
                    None => Vec::new(),
                };
                ret.insert(k.clone(), v);
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

pub async fn sync_groups(
    config: Arc<SiriusConfig>,
    identity: &Identity,
    users: &[(String, Value)],
) -> Result<()> {
    info!("recuparating groups from identity");
    let meta = match &config.opa.mode as &str {
        "admin" => &identity.metadata_admin,
        "public" => &identity.metadata_public,
        "trait" => &identity.traits,
        _ => bail!("Invalid mode! please put a valid mode (admin, public or trait) in the config"),
    };
    let groups = match meta {
        Some(ref meta) => match meta.get("group") {
            Some(grps) => grps.as_object().ok_or_else(|| anyhow!("not an object!"))?,
            None => {
                bail!("no groups in metadata!")
            }
        },
        None => {
            bail!("this organisation as no metadata!")
        }
    };
    let mut default_group_id = String::new();
    info!("recuparating default group");
    for (id, data) in groups {
        println!("data: {}", data);
        let name = data.as_str().ok_or_else(|| anyhow!("name not a string!"))?;
        if name == "default" {
            default_group_id = id.to_owned();
        }
    }
    info!("sending payload to iam!");
    for (user, role) in users {
        info!("patching user: {user}.");
        send_to_iam(&config, &default_group_id, user, role, "user").await?;
    }
    Ok(())
}

pub async fn sync_user(
    config: Arc<SiriusConfig>,
    identity: Identity,
    correlation_id: String,
    mode: SyncMode,
) {
    match sync(&config, identity, mode).await {
        Ok(_) => {
            if let Err(e) = send_to_kafka(&config.kafka, "notif", "ok".to_string(), None).await {
                if let Err(e) = send_error(&config.kafka, "error", e.deref(), &correlation_id).await
                {
                    error!("{e}");
                    return;
                }
            }
            info!("data synced successfully!");
        }
        Err(e) => {
            error!("an error has occurred when syncing data: {e}");
            let res = send_error(&config.kafka, "error", e.deref(), &correlation_id).await;
            if let Err(e) = send_to_kafka(&config.kafka, "notif", "ko", None).await {
                error!("{e}");
            }
            if let Err(e) = res {
                error!("{e}");
            }
        }
    }
}
async fn send_to_iam(
    config: &Arc<SiriusConfig>,
    id: &str,
    ressource_id: &str,
    json: &serde_json::Value,
    perm_type: &str,
) -> Result<()> {
    let mut iam_client = config
        .iam
        .client
        .clone()
        .ok_or_else(|| anyhow!("Iam client not initialized!"))?;
    let mut input = Input {
        id: id.to_owned(),
        perm_type: perm_type.to_owned(),
        resource: ressource_id.to_owned(),
        value: serde_json::to_string(&json).unwrap(),
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
    iam_client.replace_permission(request).await?;
    Ok(())
}

pub async fn sync(
    config: &Arc<SiriusConfig>,
    mut identity: Identity,
    mode: SyncMode,
) -> Result<()> {
    let id = identity.id.clone();
    let meta = match &config.opa.mode as &str {
        "admin" => &mut identity.metadata_admin,
        "public" => &mut identity.metadata_public,
        "trait" => &mut identity.traits,
        _ => bail!("Invalid mode! please put a valid mode (admin, public or trait) in the config"),
    };
    let mut projects = match meta {
        Some(ref mut meta) => match meta.get_mut("project") {
            Some(proj) => match proj.as_object() {
                Some(old_projects) => old_projects
                    .keys()
                    .map(|e| e.as_str().to_owned())
                    .collect::<Vec<String>>(),
                None => {
                    let mut ret = Vec::new();
                    let old_projects = proj
                        .as_array()
                        .ok_or_else(|| anyhow!("not an object or an array!"))?;
                    for project in old_projects.iter() {
                        ret.push(
                            project
                                .as_u64()
                                .ok_or_else(|| anyhow!("not a number"))?
                                .to_string(),
                        )
                    }
                    ret
                }
            },
            None => Vec::new(),
        },
        None => {
            bail!("this group as no metadata!");
        }
    };
    let name = match identity.traits {
        Some(ref mut traits) => traits
            .get_mut("name")
            .ok_or_else(|| anyhow!("this group as no name!"))?
            .take(),
        None => bail!("this group as no trait!"),
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
            let users = extract_sync_id(&mut identity, "user", config).await?;
            for (user, role) in users {
                let json = json!({
                    "name": name,
                    "project": projects,
                    "role": role
                });
                info!("new project list: {json}");
                info!("patching user: {user}.");
                send_to_iam(config, &user, &id, &json, "group").await?;
            }
        }
        SyncMode::User(data) => {
            for (user, role) in data {
                let json = json!({
                    "name": name,
                    "project": projects,
                    "role": role
                });
                send_to_iam(config, &user, &id, &json, "group").await?;
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
        let config = configure(None, None, None).await;
        let config = Arc::new(config);
        send_to_iam(&config, &user, &id, &json, "groups")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_sync_simple() {
        let identity = serde_json::from_str(IDENTITY_GROUP).unwrap();
        let mode = SyncMode::Project(vec!["test".to_owned(), "test".to_owned()]);
        let config = configure(None, None, None).await;
        let config = Arc::new(config);
        sync(&config, identity, mode).await.unwrap();
    }

    #[tokio::test]
    async fn test_sync_groups_simple() {
        let identity = serde_json::from_str(IDENTITY_ORG).unwrap();
        let user = &[("test".to_owned(), Value::Null)];
        let config = configure(None, None, None).await;
        let config = Arc::new(config);
        sync_groups(config, &identity, user).await.unwrap();
    }
}
