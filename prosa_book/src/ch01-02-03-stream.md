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

> The `connect_timeout` setting prevents infinite waits during connection attempts.
