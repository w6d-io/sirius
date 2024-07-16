use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, bail, Ok, Result};
use ory_kratos_client::models::Identity;
use serde_json::Value;

use tracing::{debug, error, info};

use crate::config::SiriusConfig;

fn populate_set(projects: &mut HashSet<String>, mut data: Value) -> Result<()> {
    let data = data.take();
    match data {
        Value::Object(map) => {
            if !map.is_empty() {
                for (key, _) in &map {
                    projects.insert(key.to_owned());
                }
            }
        }
        Value::Array(array) => {
            if !array.is_empty() {
                for project in &array {
                    let project = project
                        .as_str()
                        .ok_or_else(|| anyhow!(" this shoud be a string"))?;
                    projects.insert(project.to_owned());
                }
            }
        }
        _ => bail!("This should be a map or an array!"),
    }
    Ok(())
}

fn extract_projects(projects: &mut HashSet<String>, mut data: Value) -> Result<()> {
    debug!("{data:?}");
    let data = data
        .as_object_mut()
        .ok_or_else(|| anyhow!("this should be a map!"))?;
    if !data.is_empty() {
        for (_, val) in &mut *data {
            if let Some(proj) = val.get_mut("project") {
                populate_set(projects, proj.take())?;
            }
        }
    }
    Ok(())
}

pub async fn list_project_controller(
    identity: Identity,
    config: &SiriusConfig,
) -> Result<HashSet<String>> {
    let mut projects = HashSet::new();
    let meta = match &config.opa.mode as &str {
        "admin" => identity.metadata_admin,
        "public" => identity.metadata_public,
        "trait" => identity.traits,
        _ => bail!("Invalid mode! please put a valid mode (admin, public or trait) in the config"),
    };

    let mut metadata = if let Some(mut metadata) = meta {
        metadata.take()
    } else {
        error!("no metadata in this user!");
        bail!("no metadata in this user!")
    };
    if let Some(data) = metadata.get_mut("project") {
        info!("extracting project from project");
        populate_set(&mut projects, data.take())?;
    }
    if let Some(group) = metadata.get_mut("group") {
        info!("extracting project from group");
        extract_projects(&mut projects, group.take())?;
    }
    if let Some(orga) = metadata.get_mut("organisation") {
        info!("extracting project from orga");
        extract_projects(&mut projects, orga.take())?;
    }
    Ok(projects)
}

pub async fn list_controller(
    identity: Identity,
    data_type: &str,
    config: &SiriusConfig,
) -> Result<HashMap<String, String>> {
    let mut projects = HashMap::new();
    let meta = match &config.opa.mode as &str {
        "admin" => &identity.metadata_admin,
        "public" => &identity.metadata_public,
        "trait" => &identity.traits,
        _ => bail!("Invalid mode! please put a valid mode (admin, public or trait) in the config"),
    };

    let Some(metadata) = meta else {
        error!("no metadata in this user!");
        bail!("no metadata in this user!")
    };
    if let Some(data) = metadata.get(data_type) {
        info!("estracting: {data_type}");
        let data = data
            .as_object()
            .ok_or_else(|| anyhow!("this should be a map!"))?;
        if !data.is_empty() {
            for (uuid, map) in data {
                let val = map.get("name").ok_or_else(|| anyhow!("no name found !"))?;
                let name = val
                    .as_str()
                    .ok_or_else(|| anyhow!("this should be a string!"))?;
                projects.insert(uuid.to_owned(), name.to_owned());
            }
        }
    }
    Ok(projects)
}
