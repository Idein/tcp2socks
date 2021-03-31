use std::fmt;
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;

use log::*;

use crate::byte_stream::ByteStream;
use crate::connector::Connector;
use crate::model::model::*;
use crate::model::Error;
use crate::relay::{self, RelayHandle};
use crate::server_command::ServerCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(pub u32);

impl From<u32> for SessionId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "SessionId({})", self.0)
    }
}

#[derive(Debug)]
pub struct SessionHandle {
    /// client address
    addr: SocketAddr,
    /// thread performs relay bytes
    handle: thread::JoinHandle<Result<RelayHandle, Error>>,
    /// Sender to send termination messages to relay threads
    tx: SyncSender<()>,
}

impl SessionHandle {
    pub fn new(
        addr: SocketAddr,
        handle: thread::JoinHandle<Result<RelayHandle, Error>>,
        tx: SyncSender<()>,
    ) -> Self {
        Self { addr, handle, tx }
    }

    pub fn client_addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn stop(&self) {
        trace!("stop session: {}", self.addr);
        // ignore disconnected error. if the receiver is deallocated,
        // relay threads should have been terminated.
        if self.tx.send(()).is_ok() {
            // send a message to another side relay
            self.tx.send(()).ok();
        }
    }

    pub fn join(self) -> thread::Result<Result<(), Error>> {
        trace!("join session: {}", self.addr);
        match self.handle.join()? {
            Ok(relay) => relay.join(),
            Err(err) => Ok(Err(err)),
        }
    }
}

#[derive(Debug)]
pub struct Session<D, S> {
    pub id: SessionId,
    pub dst_connector: D,
    pub server_addr: SocketAddr,
    pub dst_addr: Address,
    /// termination message receiver
    rx: Arc<Mutex<mpsc::Receiver<()>>>,
    /// Send `Disconnect` command to the main thread.
    /// This guard is shared with 2 relays.
    guard: Arc<Mutex<DisconnectGuard<S>>>,
}

impl<D, S> Session<D, S>
where
    D: Connector,
    S: Send + 'static,
{
    /// Returns Self and termination message sender.
    pub fn new(
        id: SessionId,
        dst_connector: D,
        server_addr: SocketAddr,
        dst_addr: Address,
        tx_cmd: mpsc::Sender<ServerCommand<S>>,
    ) -> (Self, mpsc::SyncSender<()>) {
        let (tx, rx) = mpsc::sync_channel(2);
        (
            Self {
                id,
                dst_connector,
                server_addr,
                dst_addr,
                rx: Arc::new(Mutex::new(rx)),
                guard: Arc::new(Mutex::new(DisconnectGuard::new(id, tx_cmd))),
            },
            tx,
        )
    }

    fn make_session<'a>(
        &self,
        src_addr: SocketAddr,
        src_conn: impl ByteStream + 'a,
    ) -> Result<RelayHandle, Error> {
        info!("connect new client: dst_addr = {}", self.dst_addr);

        let (strm, proxy_addr) = match self
            .dst_connector
            .connect_byte_stream(self.dst_addr.clone())
        {
            Ok((strm, proxy_addr)) => {
                info!(
                    "connected: proxy_addr = {}, dst_addr = {}",
                    proxy_addr, self.dst_addr
                );
                (strm, proxy_addr)
            }
            Err(err) => {
                error!("connect error: {}", err);
                trace!("connect error: {:?}", err);
                return Err(err);
            }
        };

        relay::spawn_relay(
            src_addr,
            proxy_addr,
            Box::new(src_conn),
            strm,
            self.rx.clone(),
            self.guard.clone(),
        )
    }

    pub fn start<'a>(
        self,
        src_addr: SocketAddr,
        src_conn: impl ByteStream + 'a,
    ) -> Result<RelayHandle, Error> {
        self.make_session(src_addr, src_conn)
    }
}

#[derive(Debug, Clone)]
pub struct DisconnectGuard<S> {
    id: SessionId,
    tx: mpsc::Sender<ServerCommand<S>>,
}

impl<S> DisconnectGuard<S> {
    pub fn new(id: SessionId, tx: mpsc::Sender<ServerCommand<S>>) -> Self {
        Self { id, tx }
    }
}

impl<S> Drop for DisconnectGuard<S> {
    fn drop(&mut self) {
        debug!("DisconnectGuard: {}", self.id);
        self.tx.send(ServerCommand::Disconnect(self.id)).unwrap()
    }
}
