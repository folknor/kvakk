use thiserror::Error;

#[derive(Error, Debug)]
pub enum LocalSendError {
    #[error("IO error: {source}")]
    Io {
        #[from]
        source: std::io::Error,
    },

    #[error("Serde JSON error: {source}")]
    Serde {
        #[from]
        source: serde_json::Error,
    },

    #[error("HTTP client error: {source}")]
    Reqwest {
        #[from]
        source: reqwest::Error,
    },

    #[error("Address parse error: {source}")]
    AddrParse {
        #[from]
        source: std::net::AddrParseError,
    },

    #[error("Network error: {message}")]
    Network { message: String },

    #[error("Request rejected by receiver (HTTP {status})")]
    Rejected { status: u16 },

    #[error("Request failed with HTTP {status}: {message}")]
    HttpFailed { status: u16, message: String },

    #[error("Session blocked by another transfer")]
    SessionBlocked,

    #[error("Too many requests")]
    RateLimited,
}

impl LocalSendError {
    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network { message: msg.into() }
    }

    pub fn http_failed(status: u16, message: impl Into<String>) -> Self {
        Self::HttpFailed { status, message: message.into() }
    }
}

pub type Result<T> = std::result::Result<T, LocalSendError>;
