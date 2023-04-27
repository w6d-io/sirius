use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, bail, Ok, Result};
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
    Project,
}

pub async fn sync_user(
    config: Arc<SiriusConfig>,
    mut identitys: Vec<Identity>,
    request_id: String,
    mut mode: SyncMode,
) -> Result<()> {
    let mut client = config
        .iam
        .client
        .clone()
        .ok_or_else(|| anyhow!("{request_id}: Iam client not initialized!"))?;
    while let Some(mut identity) = identitys.pop() { 
        let id = identity.id.clone();
        let projects = match identity.metadata_admin {
            Some(ref mut meta) => match meta.get_mut("projects") {
                Some(proj) => proj.take(),
                None => Value::Null,
            },
            None => bail!("{request_id}: this scope as no metadata!"),
        };
        match &mut mode {
            SyncMode::Project =>{
                let users = extract_sync_id(&mut identity, &request_id, "user").await?;
                let json = json!({
                    "projects": projects
                });
                for user in users {
                    let mut input = Input {
                        id: user.to_owned(),
                        perm_type: "scope".to_owned(),
                        resource: id.clone(),
                        value: serde_json::to_string(&json).unwrap(),
                        ..Default::default()
                    };
                    input.set_mode(Mode::Meta);
                    let request = Request::new(input);
                    client.replace_permission(request).await?;
                }
            },
            SyncMode::User(data) => match data.remove(&id) {
                Some((user, role)) => {
                    let name = match identity.traits {
                        Some(ref mut traits) => {
                            traits
                                .get_mut("name")
                                .ok_or_else(|| anyhow!("{request_id}: this scope as no name!"))?
                                .take()
                        }
                        None => bail!("{request_id}: this scope as no trait!"),
                    };
                    let json = json!({
                        "name": name,
                        "projects": projects,
                        "role": role
                    });
                    let mut input = Input {
                        id: user,
                        perm_type: "scope".to_owned(),
                        resource: id.clone(),
                        value: serde_json::to_string(&json).unwrap(),
                        ..Default::default()
                    };
                    input.set_mode(Mode::Meta);
                    let request = Request::new(input);
                    client.add_permission(request).await?;
                },
                None => {
                    warn!("{request_id}: no corresponding user found!");
                }
            },
        };
    }
    Ok(())
}
