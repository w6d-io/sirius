use anyhow::Result;
use serde::Serialize;

use crate::{config::Kafka, utils::kafka::send_to_kafka};

#[derive(Serialize)]
pub struct ErrorData<'a> {
    code: &'a str,
    message: String,
}

///send error to the given kafka topic
#[cfg(not(tarpaulin_include))]
pub async fn send_error<T>(config: &Kafka, topic: &str, data: T, correlation_id: &str) -> Result<()>
where
    T: std::error::Error,
{
    let error = ErrorData {
        code: "webhook_internal_error",
        message: data.to_string(),
    };
    send_to_kafka(config, topic, &error, Some(correlation_id)).await
}
