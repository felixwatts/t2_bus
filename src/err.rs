use thiserror::Error;

#[derive(Error, Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum BusError{
    #[error("Invalid topic string: {0}")]
    InvalidTopicString(String),

    #[error("Request failed: Channel closed while awaiting ACK")]
    RequestFailedChannelClosed,
    #[error("Request failed: Timeout while awaiting ACK")]
    RequestFailedTimeout,
    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Respond failed: Invalid request ID")]
    InvalidRequestId,

    #[error("Claim topic failed: Already claimed")]       
    TopicAlreadyClaimed,
    #[error("Claim topic failed: Wildcards not supported")]    
    ClaimWildcardNotSupported,

    #[error("Channel closed by peer")]    
    ChannelClosed,

    #[error("Service not found: {0}")]    
    ServiceNotFound(String),
    #[error("Message delivery failed: Client #{0}: {1}")]
    DeliveryFailed(u32, String),
    #[error("Message delivery failed: Unknown client {0}")]    
    UnknownClient(u32),

    #[error("Deserialize message failed. Protocol: {0}. Error: {1}")]    
    MalformedMessage(String, String),
    #[error("Serialize message failed: {0}")]    
    UnserializableMessage(String),

    #[error("IO error: {0}")]    
    IOError(String),
    #[error("Internal error: {0}")]    
    InternalError(String),

    #[error("TLS configuration error: {0}")]
    TlsConfigError(String)
}

impl<T> From<ciborium::ser::Error<T>> for BusError {
    fn from(_error: ciborium::ser::Error<T>) -> Self {
        BusError::IOError("CBOR serialization failed.".into())
    }
}

impl<T> From<ciborium::de::Error<T>> for BusError {
    fn from(_error: ciborium::de::Error<T>) -> Self {
        BusError::IOError("CBOR deserialization failed.".into())
    }
}

impl From<std::io::Error> for BusError {
    fn from(error: std::io::Error) -> Self {
        BusError::IOError(error.to_string())
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for BusError {
    fn from(_error: tokio::sync::mpsc::error::SendError<T>) -> Self {
        BusError::ChannelClosed
    }
}

impl From<tokio::task::JoinError> for BusError {
    fn from(error: tokio::task::JoinError) -> Self {
        BusError::InternalError(error.to_string())
    }
}

impl From<serde_json::error::Error> for BusError {
    fn from(error: serde_json::error::Error) -> Self {
        BusError::UnserializableMessage(error.to_string())
    }
}

impl From<tokio::sync::oneshot::error::RecvError> for BusError {
    fn from(error: tokio::sync::oneshot::error::RecvError) -> Self {
        BusError::InternalError(error.to_string())
    }
}

pub type BusResult<T> = Result<T, BusError>;

