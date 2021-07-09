# 0.15.0

- Update the `tungstenite-rs` version to `0.14.0`,
  [check `tungstenite-rs` release for more details](https://github.com/snapview/tungstenite-rs/blob/master/CHANGELOG.md#0140).

# 0.14.0

- Support for `rustls` as TLS backend.
  - The `tls` feature was renamed to `native-tls` and uses a OS-native TLS implementation.
  - A new `native-tls-vendored` feature that uses `native-tls` but forces to build a vendored
    version (mostly for `openssl`) instead of linking against the system installation.
  - New `rustls-tls` feature flag to enable TLS with `rustls` as backend.
  - `stream::Stream` was renamed to `MaybeTlsStream` and wraps a `rustls` TLS stream as well now.
  - If both `native-tls` and `rustls-tls` are enabled `native-tls` is used by default.
  - A new `Connector` was introduced that is similar to the previous `TlsConnector` but now allows
    to control the used TLS backend explicitly (or disable it) in `client_async_tls_with_config`.

# 0.13.0

- Upgrade from Tokio 0.3 to Tokio 1.0.0.
