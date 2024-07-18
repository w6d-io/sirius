use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{bail, Result};
use async_trait::async_trait;
use figment::{
    providers::{Format, Toml},
    Figment,
};
use serde::Deserialize;
use tonic::transport::{Channel, Endpoint};

use kafka::producer::{
    default_config, future_producer::DefaultFutureContext, FutureProducer, KafkaProducer,
};
use rs_utils::config::{Config, Kratos};

use crate::permission::iam_client::IamClient;

pub const CONFIG_FALLBACK: &str = "test/config.toml";

/// Structure representing kafka producer config.
#[derive(Deserialize, Clone, Default)]
pub struct Producer {
    pub topics: Vec<String>,

    #[serde(skip)]
    pub clients: Option<HashMap<String, Arc<KafkaProducer<FutureProducer, DefaultFutureContext>>>>,
}

impl fmt::Debug for Producer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Producer")
            .field("topics", &self.topics)
            .finish_non_exhaustive()
    }
}

impl Producer {
    /// Update the producer if needed.
    pub fn update(&mut self, broker: &str) -> Result<()> {
        let mut new_producer = HashMap::new();
        let producer = match self.clients {
            Some(ref mut prod) => prod,
            None => &mut new_producer,
        };
        for topic in &self.topics {
            producer.insert(
                topic.to_owned(),
                Arc::new(KafkaProducer::<FutureProducer, DefaultFutureContext>::new(
                    &default_config(broker),
                    topic,
                )?),
            );
        }
        Ok(())
    }
}

/// Structure representing the kafka config.
#[derive(Deserialize, Clone, Default, Debug)]
pub struct Kafka {
    pub broker: String,
    pub producers: Producer,
}

impl Kafka {
    fn update(&mut self) -> Result<&mut Self> {
        self.producers.update(&self.broker)?;
        Ok(self)
    }
}

/// Structure representing the service port config.
#[derive(Deserialize, Clone, Default, Debug)]
pub struct Ports {
    pub main: String,
    pub health: String,
}

/// Structure representing the service config.
#[derive(Deserialize, Clone, Default, Debug)]
pub struct Service {
    pub addr: String,
    pub ports: Ports,
}

/// Structure representing the iam connection config.
#[derive(Deserialize, Clone, Default, Debug)]
pub struct Iam {
    pub service: Service,
    #[serde(skip)]
    pub client: Option<IamClient<Channel>>,
}

/// Structure representing the opa connection config.
#[derive(Deserialize, Clone, Default, Debug)]
pub struct Opa {
    pub addr: String,
    pub mode: String,
}

/// Structure containing the configuaration of the application.
#[derive(Deserialize, Clone, Default, Debug)]
pub struct SiriusConfig {
    // pub prefix: String,
    pub service: Service,
    pub iam: Iam,
    pub opa: Opa,
    pub kratos: Kratos,
    pub kafka: Kafka,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Iam {
    fn update(&mut self) -> Result<()> {
        let addr = "http://".to_string()
            + &self.service.addr as &str
            + ":"
            + &self.service.ports.main as &str;
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
        config.iam.update()?;
        config.set_path(path);
        config.kafka.update()?;
        *self = config;
        Ok(())
    }
}

#[cfg(test)]
mod test_config {
    use super::*;

    #[tokio::test]
    async fn test_update_valid() {
        let mut config = SiriusConfig::default();
        config.set_path("tests/config.toml");
        let res = config.update().await;
        assert!(res.is_ok())
    }

    #[tokio::test]
    async fn test_update_not_valid() {
        let mut config = SiriusConfig::default();
        config.set_path("tests/not_config.toml");
        let res = config.update().await;
        assert!(res.is_err())
    }

    #[tokio::test]
    async fn test_update_iam() {
        let mut config = SiriusConfig::default();
        config.iam.service.addr = "0.0.0.0".to_owned();
        config.iam.service.ports.main = "8383".to_owned();
        let res = config.iam.update().await;
        assert!(res.is_ok())
    }
}
