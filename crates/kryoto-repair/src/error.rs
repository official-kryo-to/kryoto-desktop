use thiserror::Error;
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    Tool(String),
    #[error("{0}")]
    Io(String),
    #[error("Cancelled")]
    Stopped,
}
pub fn check_cancel(cancel: &impl crate::Cancellation) -> Result<()> {
    if cancel.requested() {
        Err(Error::Stopped)
    } else {
        Ok(())
    }
}
