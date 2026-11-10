use thiserror::Error;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid request")]
    InvalidRequest,
    #[error("identity conflict")]
    IdentityConflict,
    #[error("request not found")]
    RequestNotFound,
    #[error("request already answered")]
    AlreadyAnswered,
    #[error("invalid option")]
    InvalidOption,
    #[error("message too large or incomplete")]
    InvalidFrame,
    #[error("operation timed out")]
    Timeout,
    #[error("{0}")]
    Remote(String),
    #[error("{0}")]
    Task(String),
    #[error("{0}")]
    Zmq(#[from] zmq::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    InvalidId(#[from] std::num::ParseIntError),
}
