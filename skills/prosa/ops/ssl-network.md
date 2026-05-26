# SSL & Network Configuration Reference

## SSL Library Selection

Features:
- `openssl` — use system OpenSSL (default)
- `openssl-vendored` — compile and statically link OpenSSL

## SSL Store

Trust store for CA certificates. Two options:

### Directory path (scans recursively for `.pem` and `.der` files)

```yaml
ssl:
  store:
    path: "/etc/ssl/certs/"
```

### Inline PEM certificates

```yaml
ssl:
  store:
    certs:
      - |
          -----BEGIN CERTIFICATE-----
          MIICGzCCAaGgAwIBAgIQ...
          -----END CERTIFICATE-----
```

## SslConfig

Full SSL configuration object:

```yaml
ssl:
  store:
    path: "/etc/ssl/certs/"
  cert: "/opt/cert.pem"
  key: "/opt/cert.key"
  passphrase: "key_passphrase"
  alpn:
    - "h2"
    - "http/1.1"
  modern_security: true
  ssl_timeout: 3000
```

### PKCS#12 bundle

```yaml
ssl:
  store:
    path: "/etc/ssl/certs/"
  pkcs12: "/opt/cert.p12"
  passphrase: "p12_passphrase"
```

### Self-signed certificate

If `ssl://` or `+ssl://` protocol is used without specifying cert/key, ProSA generates a self-signed certificate. To export it:

```yaml
ssl:
  cert: "/opt/self_signed_cert.pem"
```

### Usage context

- **Client-side**: store validates server certificates; cert/key used as client certificate
- **Server-side**: store validates client certificates (mTLS); cert/key used as server certificate

## Stream Listener (Server)

Configuration for accepting connections:

```yaml
listener:
  url: "0.0.0.0:8080"
  ssl:
    cert: "/opt/cert.pem"
    key: "/opt/cert.key"
    passphrase: "key_passphrase"
  max_socket: 4000000
```

- `url` — bind address and port
- `ssl` — optional SSL configuration (see above)
- `max_socket` — max concurrent connections (prevents overload)

Supported protocols: TCP, UNIX sockets, SSL/TLS.

## Stream Target (Client)

Configuration for outgoing connections:

```yaml
stream:
  url: "example.com:443"
  ssl:
    store:
      path: "/etc/ssl/certs/"
  proxy: "http://myhttpproxy:3128"
  connect_timeout: 3000
```

- `url` — target address and port
- `ssl` — optional SSL configuration
- `proxy` — HTTP proxy for tunneling (requires `http-proxy` feature)
- `connect_timeout` — connection timeout in milliseconds (prevents infinite waits)

Supported connection types: TCP, SSL/TLS, UNIX socket, TCP through HTTP proxy, SSL through HTTP proxy.
