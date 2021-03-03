use std::time::Duration;

use crate::model::{Address, SocketAddr};

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub server_addr: SocketAddr,
    pub proxy_addr: SocketAddr,
    pub dst_addr: Address,
    /// timeout of relaying data chunk from client to external network. (default: 2000ms)
    pub client_rw_timeout: Option<Duration>,
    /// timeout of relaying data chunk from external network to client. (default: 5000ms)
    pub server_rw_timeout: Option<Duration>,
    /// timeout of accpet connection from client. (default 3s)
    pub accept_timeout: Option<Duration>,
}

impl ServerConfig {
    pub fn new(server_addr: SocketAddr, proxy_addr: SocketAddr, dst_addr: Address) -> Self {
        Self {
            server_addr,
            proxy_addr,
            dst_addr,
            client_rw_timeout: Some(Duration::from_millis(2000)),
            server_rw_timeout: Some(Duration::from_millis(5000)),
            accept_timeout: Some(Duration::from_secs(3)),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig::new(
            "127.0.0.1:1081".parse().unwrap(),
            "127.0.0.1:1080".parse().unwrap(),
            "127.0.0.1:80".parse::<SocketAddr>().unwrap().into(),
        )
    }
}
