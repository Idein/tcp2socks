use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::byte_stream::ByteStream;
use crate::model::error::Error;
use crate::model::model::*;
use crate::pkt_stream::{PktStream, UdpPktStream};

use socks::Socks5Stream;

pub trait Connector: Send {
    type B: ByteStream;
    type P: PktStream;
    fn connect_byte_stream(&self, addr: Address) -> Result<(Self::B, SocketAddr), Error>;
    fn connect_pkt_stream(&self, addr: Address) -> Result<(Self::P, SocketAddr), Error>;
}

#[derive(Debug, Clone)]
pub struct SocksConnector {
    proxy_addr: SocketAddr,
    rw_timeout: Option<Duration>,
}

impl SocksConnector {
    pub fn new(proxy_addr: SocketAddr, rw_timeout: Option<Duration>) -> Self {
        Self {
            proxy_addr,
            rw_timeout,
        }
    }
}

impl Connector for SocksConnector {
    type B = TcpStream;
    type P = UdpPktStream;

    fn connect_byte_stream(&self, addr: Address) -> Result<(Self::B, SocketAddr), Error> {
        let strm = Socks5Stream::connect(self.proxy_addr, addr)?;
        let strm = strm.into_inner();
        strm.set_read_timeout(self.rw_timeout)?;
        strm.set_write_timeout(self.rw_timeout)?;

        Ok((strm, self.proxy_addr))
    }

    fn connect_pkt_stream(&self, _addr: Address) -> Result<(Self::P, SocketAddr), Error> {
        unimplemented!("connect_pkt_stream")
        /*
        let sock_addr = self.resolve(addr)?;
        UdpSocket::connect(sock_addr).map_err(Into::into)
        */
    }
}
