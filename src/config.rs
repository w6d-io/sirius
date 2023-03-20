use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use async_trait::async_trait;
use figment::{
    providers::{Format, Toml},
    Figment,
};
use serde::Deserialize;
use tonic::transport::{Channel, Endpoint};

use rs_utils::config::{Config, Kratos};

use crate::permission::iam_client::IamClient;

pub const CONFIG_FALLBACK: &str = "test/config.toml";

#[derive(Deserialize, Clone, Default, Debug)]
pub struct Ports {
    pub main: String,
    pub health: String,
}

#[derive(Deserialize, Clone, Default, Debug)]
pub struct Service {
    pub addr: String,
    pub ports: Ports,
}

#[derive(Deserialize, Clone, Default, Debug)]
pub struct Iam {
    pub service: Service,
    #[serde(skip)]
    pub client: Option<IamClient<Channel>>,
}

///structure containing the configuaration of the application
#[derive(Deserialize, Clone, Default, Debug)]
pub struct SiriusConfig {
    // pub prefix: String,
    pub service: Service,
    pub iam: Iam,
    pub kratos: Kratos,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Iam {
    async fn update(&mut self) -> Result<()> {
        let addr = "http://".to_string() + &self.service.addr as &str + ":" + &self.service.ports.main as &str;
        let endpoint = Endpoint::try_from(addr)?.connect_lazy();
        self.client = Some(IamClient::new(endpoint));
        Ok(())
    }
}

#[async_trait]
impl Config for SiriusConfig {
    fn set_path<T: AsRef<Path>>(&mut self, path: T) -> &mut Self {
        self.path = Some(path.as_ref().to_path_buf());
        self
    }
    ///update the config structure
    async fn update(&mut self) -> Result<()> {
        let path = match self.path {
            Some(ref path) => path as &Path,
            None => bail!("config file path not set"),
        };
        match path.try_exists() {
            Ok(exists) if !exists => bail!("config was not found"),
            Err(e) => bail!(e),
            _ => (),
        }
        let mut config: SiriusConfig = Figment::new().merge(Toml::file(path)).extract()?;
        config.kratos.update();
        config.iam.update().await?;
        config.set_path(path);
        *self = config;
        Ok(())
    }
}
