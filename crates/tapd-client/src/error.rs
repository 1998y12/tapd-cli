use reqwest::StatusCode;
use thiserror::Error;

/// Errors returned by the TAPD client and protocol helpers.
#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid TAPD base URL: {0}")]
    InvalidBaseUrl(#[from] url::ParseError),

    #[error("invalid authorization header")]
    InvalidAuthorizationHeader,

    #[error("failed to build HTTP client: {0}")]
    ClientBuild(#[source] reqwest::Error),

    #[error("TAPD request failed: {0}")]
    Transport(#[source] reqwest::Error),

    #[error("TAPD returned HTTP {status}: {body}")]
    Http { status: StatusCode, body: String },

    #[error("TAPD API error (status={status}): {info}")]
    Api { status: String, info: String },

    #[error("invalid TAPD JSON response: {message}")]
    Protocol { message: String },

    #[error("file operation failed for {path}: {source}")]
    File {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("attachment upload could not be verified: {0}")]
    AttachmentVerification(String),

    #[error("inline image upload could not be verified: {0}")]
    ImageVerification(String),

    #[error("invalid TAPD URL: {0}")]
    InvalidTapdUrl(String),

    #[error("workflow transition is unavailable: {0}")]
    Workflow(String),
}

pub type Result<T> = std::result::Result<T, Error>;
