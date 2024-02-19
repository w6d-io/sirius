use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use rs_utils::kratos::Identity;

use crate::config::SiriusConfig;

#[derive(Deserialize, Serialize)]
struct Input<'a> {
    uri: &'a str,
    method: &'a str,
    role: &'a str, // scop? add to option file
    resource: &'a str,
}

#[derive(Deserialize, Serialize)]
struct OpaData<'a> {
    #[serde(borrow)]
    input: Input<'a>,
    data: Identity,
}
pub async fn validate_roles(
    config: &SiriusConfig,
    identity: &Identity,
    project_id: &str,
    request_id: &str,
    uri: &str,
) -> Result<bool> {
    let input = Input {
        uri,
        method: "post",
        role: "unused", //get from conf file
        resource: project_id,
    };
    let opa = OpaData {
        input,
        data: identity.to_owned(),
    };
    let client = match &config.kratos.client {
        Some(client) => client,
        None => bail!("{request_id}: kratos client not initialized"),
    };

    let res = client
        .client
        .post(&config.opa.addr)
        .header("correlation_id", request_id)
        .json(&opa)
        .send()
        .await?;
    res.error_for_status_ref()?;
    let body = res.json::<bool>().await?;
    Ok(body)
}
