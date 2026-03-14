use thiserror::Error;

#[derive(Debug, Error)]
pub enum AwsmError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Terminal error: {0}")]
    Terminal(String),

    #[error("AWS error: {0}")]
    Aws(String),
}

pub type Result<T> = std::result::Result<T, AwsmError>;
