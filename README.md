# tcp2socks: Proxy server converting TCP to SOCKS5

## Usage

Example:

```bash
$ tcp2socksd tcp://127.0.0.1:1081 socks5h://127.0.0.1:1080 tcp://localhost:554
```

This command launch a proxy server that:

- listens TCP connection on `127.0.0.1:1081`.
- proxies the connection to SOCKS proxy on `127.0.0.1:1081`.
- routes the connection to `localhost:554`.
