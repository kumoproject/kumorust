use std::fmt;
use std::io;

/// Application-wide error type for non-UI layers.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Message(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Message(_) => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
