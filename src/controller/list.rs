use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, bail, Ok, Result};
use ory_kratos_client::models::Identity;
use serde_json::Value;

use tracing::{error, info};

use crate::config::SiriusConfig;

fn populate_set(projects: &mut HashSet<String>, mut data: Value, request_id: &str) -> Result<()> {
    let data = data.take();
    let data = data
        .as_object()
        .ok_or_else(|| anyhow!("{request_id}: This should be a map!"))?;
    if !data.is_empty() {
        for (key, _) in data.iter() {
            projects.insert(key.to_owned());
        }
    }
    Ok(())
}

fn extract_projects(
    projects: &mut HashSet<String>,
    mut data: Value,
    request_id: &str,
) -> Result<()> {
    let data = data
        .as_object_mut()
        .ok_or_else(|| anyhow!("this should be a map!"))?;
    if !data.is_empty() {
        for (_, val) in data.into_iter() {
            if let Some(proj) = val.get_mut("project") {
                populate_set(projects, proj.take(), request_id)?;
            }
        }
    }
    Ok(())
}

pub async fn list_project_controller(
    request_id: &str,
    identity: Identity,
    config: SiriusConfig,
) -> Result<HashSet<String>> {
    let mut projects = HashSet::new();
    let meta = match &config.mode as &str {
        "admin" => identity.metadata_admin,
        "public" => identity.metadata_public,
        "trait" => identity.traits,
        _ => bail!("Invalid mode! please put a valid mode (admin, public or trait) in the config"),
    };

    let mut metadata = match meta {
        Some(mut metadata) => metadata.take(),
        None => {
            error!("{request_id}: no metadata in this user!");
            bail!("{request_id}: no metadata in this user!")
        }
    };
    if let Some(data) = metadata.get_mut("project") {
        info!("{request_id}: extracting project from project");
        populate_set(&mut projects, data.take(), request_id)?;
    }
    if let Some(group) = metadata.get_mut("group") {
        info!("{request_id}: extracting project from group");
        extract_projects(&mut projects, group.take(), request_id)?;
    }
    if let Some(orga) = metadata.get_mut("organisation") {
        info!("{request_id}: extracting project from orga");
        extract_projects(&mut projects, orga.take(), request_id)?;
    }
    Ok(projects)
}

pub async fn list_controller(
    request_id: &str,
    identity: Identity,
    data_type: &str,
    config: SiriusConfig,
) -> Result<HashMap<String, String>> {
    let mut projects = HashMap::new();
    let meta = match &config.mode as &str {
        "admin" => &identity.metadata_admin,
        "public" => &identity.metadata_public,
        "trait" => &identity.traits,
        _ => bail!("Invalid mode! please put a valid mode (admin, public or trait) in the config"),
    };

    let metadata = match meta {
        Some(metadata) => metadata,
        None => {
            error!("{request_id}: no metadata in this user!");
            bail!("{request_id}: no metadata in this user!")
        }
    };
    if let Some(data) = metadata.get(data_type) {
        info!("{request_id}: estracting: {data_type}");
        let data = data
            .as_object()
            .ok_or_else(|| anyhow!("this should be a map!"))?;
        if !data.is_empty() {
            for (uuid, map) in data.iter() {
                let val = map
                    .get("name")
                    .ok_or_else(|| anyhow!("{request_id}: no name found !"))?;
                let name = val
                    .as_str()
                    .ok_or_else(|| anyhow!("{request_id}: this should be a string!"))?;
                projects.insert(uuid.to_owned(), name.to_owned());
            }
        }
    }
    Ok(projects)
}
