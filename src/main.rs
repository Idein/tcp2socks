//! Proxy server converting TCP to SOCKS5

#[macro_use]
extern crate eyre;

use ::clap::App;
use color_eyre::Section;
use eyre::{Result, WrapErr};
use log::*;
use std::io;
use std::net::SocketAddr;
use tcp2socks::model::model::Address;
use url::Url;

fn parse_url(s: &str) -> Result<Url> {
    Url::parse(s).wrap_err_with(|| eyre!("invalid URL: {}", s))
}

fn validate_socket_addr_contained_unique(url: &Url) -> Result<()> {
    url.socket_addrs(|| None)
        .wrap_err_with(|| {
            format!(
                "endpoint url must be the form of `protocol://host:port`: {}",
                url
            )
        })
        .and_then(|xs| {
            if xs.len() == 1 {
                Ok(())
            } else {
                Err(eyre!("socket address must be unique."))
            }
        })
}

#[derive(Debug)]
pub struct Pipeline {
    src: ServerUrl,
    proxy: ProxyUrl,
    dst: DestinationUrl,
}

impl Pipeline {
    pub fn parse(args: Vec<&str>) -> Result<Self> {
        if args.len() != 3 {
            return Err(eyre!("pipeline must be length 3. (src, proxy, dst)"));
        }

        match args.as_slice() {
            [src, proxy, dst] => {
                let src = ServerUrl::new(parse_url(src)?)?;
                let proxy = ProxyUrl::new(parse_url(proxy)?)?;
                let dst = DestinationUrl::new(parse_url(dst)?)?;
                Ok(Self { src, proxy, dst })
            }
            _ => unreachable!(),
        }
    }

    pub fn server_addr(&self) -> SocketAddr {
        self.src.socket_addr()
    }

    pub fn proxy_addr(&self) -> SocketAddr {
        self.proxy.socket_addr()
    }

    pub fn dst_addr(&self) -> Address {
        self.dst.addr()
    }
}

#[derive(Debug, Clone)]
struct ServerUrl(SocketAddr);

#[derive(Debug, Clone)]
struct ProxyUrl(SocketAddr);

#[derive(Debug, Clone)]
struct DestinationUrl(Address);

impl ServerUrl {
    pub fn new(url: Url) -> Result<Self> {
        if url.scheme() != "tcp" {
            return Err(eyre!("not supportted server protocol: url = {}", url))
                .note("supported protocols: tcp");
        }

        validate_socket_addr_contained_unique(&url)?;
        let addr = url.socket_addrs(|| None).unwrap().pop().unwrap();

        Ok(Self(addr))
    }

    pub fn socket_addr(&self) -> SocketAddr {
        self.0
    }
}

impl ProxyUrl {
    pub fn new(url: Url) -> Result<Self> {
        const PROTOCOLS: &[&str] = &["socks5h"];

        if !PROTOCOLS.contains(&url.scheme()) {
            return Err(eyre!("not supportted proxy protocol: url = {}", url))
                .note("supported protocols: socks5h");
        }

        validate_socket_addr_contained_unique(&url)?;
        let addr = url.socket_addrs(|| None).unwrap().pop().unwrap();

        Ok(Self(addr))
    }

    pub fn socket_addr(&self) -> SocketAddr {
        self.0
    }
}

impl DestinationUrl {
    pub fn new(url: Url) -> Result<Self> {
        if url.scheme() != "tcp" {
            return Err(eyre!("not supportted destination protocol: url = {}", url))
                .note("supported protocols: tcp");
        }

        let addr = match (url.host(), url.port()) {
            (Some(host), Some(port)) => {
                use url::Host as H;
                match host {
                    H::Domain(domain) => Address::Domain(domain.into(), port),
                    H::Ipv4(ip) => Address::IpAddr(ip.into(), port),
                    H::Ipv6(ip) => Address::IpAddr(ip.into(), port),
                }
            }
            _ => {
                return Err(eyre!(
                    "destination url should be `tcp://<host>:<port>`: url = {}",
                    url
                ));
            }
        };

        Ok(Self(addr))
    }

    pub fn addr(&self) -> Address {
        self.0.clone()
    }
}

fn set_handler(signals: &[i32], handler: impl Fn(i32) + Send + 'static) -> io::Result<()> {
    use signal_hook::*;
    let signals = iterator::Signals::new(signals)?;
    std::thread::spawn(move || signals.forever().for_each(handler));
    Ok(())
}

fn main() -> eyre::Result<()> {
    use signal_hook::*;

    pretty_env_logger::init_timed();
    color_eyre::install()?;

    let yaml = ::clap::load_yaml!("cli.yaml");
    let app = App::from(yaml).version(::clap::crate_version!());
    let matches = app.get_matches();

    let pipeline = matches.values_of("url").expect("required").collect();
    let pipeline = Pipeline::parse(pipeline)?;
    let config = tcp2socks::ServerConfig::new(
        pipeline.server_addr(),
        pipeline.proxy_addr(),
        pipeline.dst_addr(),
    );

    let (mut server, tx) = tcp2socks::server::Server::new(config);
    set_handler(&[SIGTERM, SIGINT, SIGQUIT, SIGCHLD], move |_| {
        tx.send(tcp2socks::ServerCommand::Terminate).ok();
    })
    .expect("setting ctrl-c handler");

    match server.serve() {
        Ok(_) => Ok(()),
        Err(err) => {
            error!("server error: {:?}", err);
            Err(eyre!("server quited with error: {:?}", err))
        }
    }
}
