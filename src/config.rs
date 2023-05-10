use std::{path::{Path, PathBuf}, fmt, collections::HashMap};

use anyhow::{bail, Result};
use async_trait::async_trait;
use figment::{
    providers::{Format, Toml},
    Figment,
};
use serde::Deserialize;
use tonic::transport::{Channel, Endpoint};

use rs_utils::config::{Config, Kratos};
use kafka::producer::{KafkaProducer, FutureProducer, default_config, future_producer::DefaultFutureContext};

use crate::permission::iam_client::IamClient;

pub const CONFIG_FALLBACK: &str = "test/config.toml";

///structure containing kafka consumer data
#[derive(Deserialize,Clone, Default)]
pub struct Producer {
    pub broker: String,
    pub topic: String,

    #[serde(skip)]
    pub client: Option<KafkaProducer<FutureProducer, DefaultFutureContext>>,
}

impl fmt::Debug for Producer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let client = match self.client {
        Some(_) => "Some(_)",
        None => "None"
    };
     write!(f,
               "Consumer: {{
                   brokers: {},
                   topic: {},
                   client: {}
               }}",
               self.broker,
               self.topic,
               client
               )
    }
}


#[derive(Deserialize,Clone, Default, Debug)]
pub struct Kafka {
    pub producers: HashMap<String, Producer>,
}

impl Kafka {
    fn update(&mut self) -> Result<&mut Self> {
        let producers = &mut self.producers;
        for producer in producers.values_mut(){
            let new_producer: KafkaProducer<FutureProducer, DefaultFutureContext> = KafkaProducer::<FutureProducer, DefaultFutureContext>::new(
                &default_config(&producer.broker),
                &producer.topic,
            )?;
            producer.client = Some(new_producer);
        }
        Ok(self)
    }
}

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
    pub opa: String,
    pub kratos: Kratos,
    pub kafka: Kafka,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Iam {
    async fn update(&mut self) -> Result<()> {
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
        config.iam.update().await?;
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
        config.set_path("test/config.toml");
        let res = config.update().await;
        assert!(res.is_ok())
    }

    #[tokio::test]
    async fn test_update_not_valid() {
        let mut config = SiriusConfig::default();
        config.set_path("test/not_config.toml");
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
