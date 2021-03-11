use std::fmt;
use std::net::ToSocketAddrs;
pub use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::str::FromStr;

use serde::*;

/// ip address and port
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Address {
    IpAddr(IpAddr, u16),
    Domain(String, u16),
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Address::*;
        match self {
            IpAddr(addr, port) => write!(f, "{}:{}", addr, port),
            Domain(host, port) => write!(f, "{}:{}", host, port),
        }
    }
}

impl Address {
    pub fn port(&self) -> u16 {
        match self {
            Address::IpAddr(_, port) => *port,
            Address::Domain(_, port) => *port,
        }
    }
}

impl From<SocketAddr> for Address {
    fn from(addr: SocketAddr) -> Self {
        Address::IpAddr(addr.ip(), addr.port())
    }
}

impl From<SocketAddrV4> for Address {
    fn from(addr: SocketAddrV4) -> Self {
        Address::IpAddr(addr.ip().clone().into(), addr.port())
    }
}

impl From<SocketAddrV6> for Address {
    fn from(addr: SocketAddrV6) -> Self {
        Address::IpAddr(addr.ip().clone().into(), addr.port())
    }
}

impl FromStr for Address {
    type Err = std::net::AddrParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let addr: SocketAddr = s.parse()?;
        Ok(addr.into())
    }
}

impl ToSocketAddrs for Address {
    type Iter = std::vec::IntoIter<SocketAddr>;

    /// Convert an address and AddrType to a SocketAddr
    fn to_socket_addrs(&self) -> std::io::Result<Self::Iter> {
        use Address::*;
        match self {
            IpAddr(ipaddr, port) => Ok(vec![SocketAddr::new(*ipaddr, *port)].into_iter()),
            Domain(domain, port) => Ok((domain.as_str(), *port).to_socket_addrs()?),
        }
    }
}

impl socks::ToTargetAddr for Address {
    fn to_target_addr(&self) -> std::io::Result<socks::TargetAddr> {
        match self {
            Address::IpAddr(std::net::IpAddr::V4(ipaddr), port) => {
                (*ipaddr, *port).to_target_addr()
            }
            Address::IpAddr(std::net::IpAddr::V6(ipaddr), port) => {
                (*ipaddr, *port).to_target_addr()
            }
            Address::Domain(domain, port) => (domain.as_str(), *port).to_target_addr(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum L4Protocol {
    Tcp,
    Udp,
}

impl fmt::Display for L4Protocol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            L4Protocol::Tcp => write!(f, "Tcp"),
            L4Protocol::Udp => write!(f, "Udp"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UdpDatagram<'a> {
    pub frag: u8,
    pub dst_addr: Address,
    pub data: &'a [u8],
}
