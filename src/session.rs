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

#[cfg(test)]
mod test {
    use super::*;
    use crate::auth_service::test::RejectService;
    use crate::byte_stream::test::BufferStream;
    use crate::connector::test::BufferConnector;
    use crate::rw_socks_stream as socks;
    use std::io;
    use std::iter::FromIterator;
    use std::str::FromStr;

    #[test]
    fn no_acceptable_method() {
        let (tx, _rx) = mpsc::channel::<ServerCommand<()>>();
        let (session, _) = Session::new(
            0.into(),
            5.into(),
            BufferConnector::from_iter(vec![(
                "192.168.0.1:5123".parse().unwrap(),
                Ok(BufferStream::new()),
            )]),
            RejectService,
            "0.0.0.0:1080".parse().unwrap(),
            ConnectRule::any(),
            tx,
        );
        println!("session: {:?}", session);
        let src = BufferStream::with_buffer(vec![5, 1, 0].into(), vec![].into());
        assert_eq!(
            session
                .make_session("192.168.0.2:12345".parse().unwrap(), src)
                .unwrap_err()
                .kind(),
            &ErrorKind::NoAcceptableMethod
        );
    }

    #[test]
    fn command_not_supported() {
        use crate::auth_service::NoAuthService;
        let mcand = MethodCandidates::new(&[Method::NoAuth]);
        // udp is not unsupported
        let req = ConnectRequest::udp_associate(Address::from_str("192.168.0.1:5123").unwrap());
        let (tx, _rx) = mpsc::channel::<ServerCommand<()>>();
        let (session, _) = Session::new(
            1.into(),
            5.into(),
            BufferConnector::from_iter(vec![(req.connect_to.clone(), Ok(BufferStream::new()))]),
            NoAuthService::new(),
            "0.0.0.0:1080".parse().unwrap(),
            ConnectRule::any(),
            tx,
        );
        println!("session: {:?}", session);

        let buff = {
            let mut cursor = io::Cursor::new(vec![]);
            socks::test::write_method_candidates(&mut cursor, mcand).unwrap();
            socks::test::write_connect_request(&mut cursor, req).unwrap();
            cursor.into_inner()
        };
        let src = BufferStream::with_buffer(buff.into(), vec![].into());
        assert_eq!(
            session
                .make_session("192.168.1.1:34567".parse().unwrap(), src)
                .unwrap_err()
                .kind(),
            &ErrorKind::command_not_supported(Command::UdpAssociate)
        );
    }

    #[test]
    fn connect_not_allowed() {
        use crate::auth_service::NoAuthService;
        let version: ProtocolVersion = 5.into();
        let connect_to = Address::from_str("192.168.0.1:5123").unwrap();
        let (tx, _rx) = mpsc::channel::<ServerCommand<()>>();
        let (session, _) = Session::new(
            2.into(),
            version,
            BufferConnector::from_iter(vec![(connect_to.clone(), Ok(BufferStream::new()))]),
            NoAuthService::new(),
            "0.0.0.0:1080".parse().unwrap(),
            ConnectRule::none(),
            tx,
        );
        println!("session: {:?}", session);

        let buff = {
            let mut cursor = io::Cursor::new(vec![]);
            socks::test::write_method_candidates(
                &mut cursor,
                MethodCandidates::new(&[Method::NoAuth]),
            )
            .unwrap();
            socks::test::write_connect_request(
                &mut cursor,
                ConnectRequest::connect_to(connect_to.clone()),
            )
            .unwrap();
            cursor.into_inner()
        };
        let src = BufferStream::with_buffer(buff.into(), vec![].into());
        assert_eq!(
            session
                .make_session("192.168.1.1:34567".parse().unwrap(), src)
                .unwrap_err()
                .kind(),
            &ErrorKind::connection_not_allowed(connect_to, L4Protocol::Tcp)
        );
    }

    #[test]
    fn connection_refused() {
        use crate::auth_service::NoAuthService;
        let version: ProtocolVersion = 5.into();
        let connect_to = Address::from_str("192.168.0.1:5123").unwrap();
        let (tx, _rx) = mpsc::channel::<ServerCommand<()>>();
        let (session, _) = Session::new(
            3.into(),
            version,
            BufferConnector::<BufferStream>::from_iter(vec![(
                connect_to.clone(),
                Err(ConnectError::ConnectionRefused),
            )]),
            NoAuthService::new(),
            "0.0.0.0:1080".parse().unwrap(),
            ConnectRule::any(),
            tx,
        );
        println!("session: {:?}", session);

        let buff = {
            let mut cursor = io::Cursor::new(vec![]);
            socks::test::write_method_candidates(
                &mut cursor,
                MethodCandidates::new(&[Method::NoAuth]),
            )
            .unwrap();
            socks::test::write_connect_request(
                &mut cursor,
                ConnectRequest::connect_to(connect_to.clone()),
            )
            .unwrap();
            cursor.into_inner()
        };
        let src = BufferStream::with_buffer(buff.into(), vec![].into());
        assert_eq!(
            session
                .make_session("192.168.1.1:34567".parse().unwrap(), src)
                .unwrap_err()
                .kind(),
            &ErrorKind::connection_refused(connect_to, L4Protocol::Tcp)
        );
    }

    fn gen_random_vec(size: usize) -> Vec<u8> {
        use rand::distributions::Standard;
        use rand::{thread_rng, Rng};
        let rng = thread_rng();
        rng.sample_iter(Standard).take(size).collect()
    }

    fn vec_from_read<T: io::Read>(mut reader: T) -> Vec<u8> {
        let mut buff = vec![];
        reader.read_to_end(&mut buff).unwrap();
        buff
    }

    #[test]
    fn relay_contents() {
        use crate::auth_service::NoAuthService;
        use io::Write;

        let version: ProtocolVersion = 5.into();
        let connect_to = Address::Domain("example.com".into(), 5123);
        let (tx, _rx) = mpsc::channel::<ServerCommand<()>>();
        let (session, _tx_session_term) = Session::new(
            4.into(),
            version,
            BufferConnector::from_iter(vec![(
                connect_to.clone(),
                Ok(BufferStream::with_buffer(
                    gen_random_vec(8200).into(),
                    vec![].into(),
                )),
            )]),
            NoAuthService::new(),
            "0.0.0.0:1080".parse().unwrap(),
            ConnectRule::any(),
            tx,
        );

        // length of SOCKS message (len MethodCandidates + len ConnectRequest)
        let input_stream_pos;
        let src = {
            // input from socks client
            let mut cursor = io::Cursor::new(vec![]);
            socks::test::write_method_candidates(
                &mut cursor,
                MethodCandidates::new(&[Method::NoAuth]),
            )
            .unwrap();
            socks::test::write_connect_request(
                &mut cursor,
                ConnectRequest::connect_to(connect_to.clone()),
            )
            .unwrap();
            input_stream_pos = cursor.position();
            // binaries from client
            cursor.write_all(&gen_random_vec(8200)).unwrap();
            BufferStream::with_buffer(cursor.into_inner().into(), vec![].into())
        };
        let dst_connector = session.dst_connector.clone();
        // start relay
        let relay = session
            .make_session("192.168.1.2:33333".parse().unwrap(), src.clone())
            .unwrap();
        assert!(relay.join().is_ok());

        // check for replied command from Session to client
        {
            // read output buffer from pos(0)
            src.wr_buff().set_position(0);
            assert_eq!(
                socks::test::read_method_selection(&mut *src.wr_buff()).unwrap(),
                MethodSelection {
                    version,
                    method: Method::NoAuth
                }
            );
            assert_eq!(
                socks::test::read_connect_reply(&mut *src.wr_buff()).unwrap(),
                ConnectReply {
                    version,
                    connect_result: Ok(()),
                    server_addr: Address::IpAddr("0.0.0.0".parse().unwrap(), 1080),
                }
            );
        }

        // check for relayed contents
        // client <-- target
        assert_eq!(vec_from_read(&mut *src.wr_buff()), {
            let mut rd_buff = dst_connector.stream(&connect_to).rd_buff();
            rd_buff.set_position(0);
            vec_from_read(&mut *rd_buff)
        });
        // client --> target
        assert_eq!(
            {
                let mut rd_buff = src.rd_buff();
                rd_buff.set_position(input_stream_pos);
                vec_from_read(&mut *rd_buff)
            },
            {
                let mut wr_buff = dst_connector.stream(&connect_to).wr_buff();
                wr_buff.set_position(0);
                vec_from_read(&mut *wr_buff)
            }
        );
    }
}
