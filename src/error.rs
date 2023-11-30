use axum::{
    http::{header::ToStrError, StatusCode},
    response::{IntoResponse, Response},
};
use thiserror::Error;
use tracing::error;

///handler for error in the http service
///it convert the recevied error in a response
#[derive(Error, Debug)]
pub enum RouterError {
    #[error("failed to serialize data.")]
    Serialisation(#[from] serde_json::Error),
    #[error("failed to apply identity patch.")]
    Internal(#[from] anyhow::Error),
    #[error("failled to convert to string.")]
    StrConvert(#[from] ToStrError),
    #[error("the request failed.")]
    Http(#[from] reqwest::Error),
    #[error("extract error.")]
    Status(StatusCode),
}

#[cfg(not(tarpaulin_include))]
impl IntoResponse for RouterError {
    fn into_response(self) -> Response {
        match self {
            RouterError::Serialisation(e) => {
                error!("{:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_SERVER_ERROR").into_response()
            }
            RouterError::Internal(e) => {
                error!("{:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_SERVER_ERROR").into_response()
            }
            RouterError::StrConvert(e) => {
                error!("{:?}, while converting str", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_SERVER_ERROR").into_response()
            }
            RouterError::Status(e) => {
                error!("status error: {:?}", e);
                (e, format!("{:?}", e.canonical_reason())).into_response()
            }
            RouterError::Http(e) => {
                error!("http error: {:?}", e);
                (StatusCode::SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE").into_response()
            }
        }
    }
}
