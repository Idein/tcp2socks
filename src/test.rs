#[cfg(test)]
mod tests {
    use crate::config::ServerConfig;
    use crate::server::Server;
    use crate::server_command::ServerCommand;
    use eyre::Result;
    use gatekeeper as gk;
    use rand::seq::SliceRandom;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::thread::{self, JoinHandle};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn start_oneshot_echo_server(port: u16) -> JoinHandle<()> {
        async fn start(port: u16) -> Result<()> {
            let listener = TcpListener::bind(("127.0.0.1", port)).await?;

            let (mut socket, _) = listener.accept().await?;

            let mut buf = [0_u8; 1024];

            // In a loop, read data from the socket and write the data back.
            loop {
                let n = match socket.read(&mut buf).await {
                    // socket closed
                    Ok(n) if n == 0 => return Ok(()),
                    Ok(n) => n,
                    Err(e) => {
                        eprintln!("failed to read from socket; err = {:?}", e);
                        return Ok(());
                    }
                };

                // Write the data back
                if let Err(e) = socket.write_all(&buf[0..n]).await {
                    eprintln!("failed to write to socket; err = {:?}", e);
                    return Ok(());
                }
            }
        }

        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(start(port)).unwrap();
        })
    }

    fn random_string(size: usize) -> String {
        const CANDIDATES: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

        let mut rng = &mut rand::thread_rng();
        let chars = CANDIDATES
            .as_bytes()
            .choose_multiple(&mut rng, size)
            .cloned()
            .collect();
        String::from_utf8(chars).unwrap()
    }

    #[test]
    #[ignore]
    fn test_proxy() -> Result<()> {
        const SRC_PORT: u16 = 11081;
        const PROXY_PORT: u16 = 11080;
        const DST_PORT: u16 = 10080;

        let handle_echo = start_oneshot_echo_server(DST_PORT);
        let config = gk::ServerConfig::new(
            "127.0.0.1".parse().unwrap(),
            PROXY_PORT,
            gk::ConnectRule::any(),
        );
        let (mut server, tx_gk) = gk::server::Server::new(config);
        let handle_gk = thread::spawn(move || {
            server.serve().unwrap();
        });
        let config = ServerConfig::new(
            format!("127.0.0.1:{}", SRC_PORT).parse().unwrap(),
            format!("127.0.0.1:{}", PROXY_PORT).parse().unwrap(),
            format!("127.0.0.1:{}", DST_PORT).parse().unwrap(),
        );
        let (mut server, tx_ts) = Server::new(config);
        let handle_ts = thread::spawn(move || {
            server.serve().unwrap();
        });

        // Wait starting servers.
        thread::sleep(Duration::from_secs(1));

        let s = random_string(128);
        let mut buf = [0; 1024];
        let mut stream = TcpStream::connect(("127.0.0.1", SRC_PORT))?;
        stream.write(s.as_bytes())?;
        let n = stream.read(&mut buf)?;
        let got = String::from_utf8((&buf[..n]).to_vec()).unwrap();
        assert_eq!(s, got);

        tx_gk.send(gk::ServerCommand::Terminate).ok();
        tx_ts.send(ServerCommand::Terminate).ok();
        handle_echo.join().unwrap();
        handle_gk.join().unwrap();
        handle_ts.join().unwrap();

        Ok(())
    }
}
