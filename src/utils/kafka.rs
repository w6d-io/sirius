#[cfg(not(test))]
use std::time::Duration;

use anyhow::Result;
#[cfg(not(test))]
use anyhow::{anyhow, bail};
use serde::Serialize;
use std::collections::HashMap;
use tracing::info;

use crate::config::Kafka;

///Send data to kafka.
#[cfg(not(tarpaulin_include))]
pub async fn send_to_kafka<T: Serialize>(
    _config: &Kafka,
    _topic: &str,
    data: T,
    header: Option<&str>,
) -> Result<()> {
    let header =
        header.map(|header| HashMap::from([("correlation_id".to_owned(), header.to_owned())]));
    let _message = kafka::KafkaMessage {
        headers: header,
        key: None,
        payload: serde_json::to_string(&data)?,
    };
    #[cfg(not(test))]
    match &_config.producers.clients {
        Some(clients) => {
            clients
                .get(_topic)
                .ok_or_else(|| anyhow!("failed to get asked kafka topic!"))?
                .produce(_message, Some(Duration::from_secs(30)))
                .await?
        }
        None => bail!("topic not found"),
    }
    info!("data successfully sent");
    Ok(())
}

#[cfg(test)]
mod test_kafka {
    use std::collections::HashMap;

    use rs_utils::config::Config;

    use super::*;
    use crate::config::SiriusConfig;

    #[tokio::test]
    async fn test_send_to_kafka() {
        let map = HashMap::from([("examples".to_owned(), 42)]);
        let config = SiriusConfig::new("tests/config.toml").await;
        assert!(
            send_to_kafka(&config.kafka, "examples", &map, Some("bonjour"))
                .await
                .is_ok()
        );
    }
}
