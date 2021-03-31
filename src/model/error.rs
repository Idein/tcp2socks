use std::fmt;
use std::fmt::Display;
use std::sync;

use failure::{Backtrace, Context, Fail};

use crate::model::*;

pub type Result<T> = ::std::result::Result<T, Error>;

#[derive(Fail, Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    #[fail(display = "io error")]
    Io,
    #[fail(display = "poisoned error: {}", _0)]
    Poisoned(String),
    #[fail(display = "disconnected channel error: {}", name)]
    Disconnected { name: String },
    #[fail(display = "packet size limit exceeded: {} > {}", size, limit)]
    PacketSizeLimitExceeded { size: usize, limit: usize },
    #[fail(display = "address already in use: {}", addr)]
    AddressAlreadInUse { addr: SocketAddr },
    #[fail(display = "address not available: {}", addr)]
    AddressNotAvailable { addr: SocketAddr },
}

impl ErrorKind {
    pub fn disconnected<S: Into<String>>(name: S) -> Self {
        ErrorKind::Disconnected { name: name.into() }
    }
}

#[derive(Debug)]
pub struct Error {
    inner: Context<ErrorKind>,
}

impl Fail for Error {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl Error {
    pub fn new(inner: Context<ErrorKind>) -> Error {
        Error { inner }
    }

    pub fn kind(&self) -> &ErrorKind {
        self.inner.get_context()
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Error {
        Error {
            inner: Context::new(kind),
        }
    }
}

impl From<Context<ErrorKind>> for Error {
    fn from(inner: Context<ErrorKind>) -> Error {
        Error { inner }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error {
            inner: error.context(ErrorKind::Io),
        }
    }
}

impl<T: fmt::Debug> From<sync::PoisonError<T>> for Error {
    fn from(error: sync::PoisonError<T>) -> Self {
        ErrorKind::Poisoned(format!("{:?}", error)).into()
    }
}
