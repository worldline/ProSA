# Stream

The `Stream` objects have been developed to make socket handling more accessible, with a high level of customization.

## Listener

For [stream listener](https://docs.rs/prosa/latest/prosa/io/listener/enum.StreamListener.html), you can use [`ListenerSetting`](https://docs.rs/prosa/latest/prosa/io/listener/struct.ListenerSetting.html) to configure it.

As a server, you need to specify the URL and optionally [SSL](ch01-02-02-ssl.md).
```yaml
listener:
  url: "0.0.0.0:8080"
  ssl:
    cert: "/opt/cert.pem"
    key: "/opt/cert.key"
    passphrase: "key_passphrase"
  max_socket: 4000000
```

> Some server implementations may support the `max_socket` parameter to prevent overload conditions.

When listener settings are logged or formatted, URL credentials are masked and query parameters
and fragments are omitted. URL paths are preserved, so they should not contain secrets.
`ListenerSetting::get_safe_url()` returns a borrowed
[`SafeUrl`](https://docs.rs/prosa/latest/prosa/io/struct.SafeUrl.html), allowing callers to format
the masked view directly, create an owned URL without credentials with `to_url()`, or create an
owned URL with masked credentials using `to_mask_url()`.

## Client

For clients, [`Stream`](https://docs.rs/prosa/latest/prosa/io/stream/enum.Stream.html) typically uses [`TargetSetting`](https://docs.rs/prosa/latest/prosa/io/stream/struct.TargetSetting.html) for configuration.

You need to specify the URL and optionally [SSL](ch01-02-02-ssl.md).
Additionally, you can specify a proxy if needed:
```yaml
stream:
  url: "worldline.com:443"
  ssl:
    store:
      path: /etc/ssl/certs/
  proxy: "http://myhttpproxy"
  connect_timeout: 3000
```

> The `connect_timeout` setting covers address resolution and connection establishment, including
> proxy and TLS handshakes.

Target and proxy URLs are redacted when settings or connection errors are formatted: credentials
are masked, while query parameters and fragments are omitted. URL paths are preserved and should
not contain secrets. `TargetSetting::get_safe_url()` returns the same borrowed `SafeUrl` view, so
the caller chooses whether to format it or convert it into either form of owned sanitized URL.

## SSL configuration of a listener or a target

Read the SSL configuration with `ssl()`, and change it with `set_ssl()` or `set_alpn()`.

`set_alpn()` creates a default SSL configuration when the listener or the target uses SSL but has
none, and does nothing on a plain one. `is_ssl()` tells whether SSL applies: an SSL configuration
**or** an SSL URL scheme is enough, so a plain `tcp://` URL with an explicit `ssl` block does
negotiate ALPN.

The OpenSSL context is built from that configuration every time a listener binds or a target
connects, so a certificate or a CA rotated on disk applies to the next bind or connection without
needing a configuration change.

Both settings implement `PartialEq`, and `set_alpn()` is idempotent, so normalise before comparing.

A target reconnects, so on a configuration reload compare the new settings with the current ones
and reconnect only when they differ.

A listener owns a bound socket, and rebinding it releases the port: another process can take it,
and every client is refused until the new socket is bound. So only change the socket when the
listener has to listen somewhere else, which is what `needs_rebind()` answers by comparing the host
and the port. Everything else is served on the socket that is already bound: build the new SSL
parameters with `build_handshaker()`, then hand them to `set_handshaker()`, which moves the socket
into the returned listener. That covers rotating a certificate, turning SSL on and turning SSL off,
whether SSL is declared by the `ssl` block or by the URL scheme.

```rust,ignore
listener_setting.set_alpn(vec!["h2".into()]);
if self.settings.listener.needs_rebind(&listener_setting) {
    listener = listener_setting.bind().await?;
} else {
    // Built before the listener is touched, so a broken certificate leaves it serving the one it
    // already has
    let handshaker = listener_setting.build_handshaker().await?;
    listener = listener.set_handshaker(handshaker);
}
```

Call it on every configuration reload of a listener, not only when the settings differ: `SslConfig`
holds the *paths* of the certificates, so a rotation that rewrites a file in place leaves the
configuration equal to what it was and there is nothing to compare. `build_handshaker()` reads them
again on every call, on the blocking pool.

A listener that is SSL through its URL scheme alone is served a default SSL configuration, which
signs a certificate of its own rather than reading one. That certificate is signed again on every
call, so such a listener serves a new identity on every configuration reload and a client that pins
it stops trusting it. Configure a certificate to serve a stable one.

The clients that are already connected, and the ones in the middle of their handshake, keep the
parameters they started with; only the clients accepted afterwards are served the new ones.
