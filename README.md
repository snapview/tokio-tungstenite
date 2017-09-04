# tokio-tungstenite

Asynchronous WebSockets for Tokio stack.

[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)
[![Crates.io](https://img.shields.io/crates/v/tokio-tungstenite.svg?maxAge=2592000)](https://crates.io/crates/tokio-tungstenite)
[![Build Status](https://travis-ci.org/snapview/tokio-tungstenite.svg?branch=master)](https://travis-ci.org/snapview/tokio-tungstenite)

[Documentation](https://docs.rs/tokio-tungstenite)

## Usage

First, you need to add this in your `Cargo.toml`:

```toml
[dependencies]
tokio-tungstenite = "*"
```

Next, add this to your crate:

```rust
extern crate tokio_tungstenite;
```

Take a look at the `examples/` directory for client and server examples. You may also want to get familiar with
[tokio](https://tokio.rs/) if you don't have any experience with it.

## What is tokio-tungstenite?

This crate is based on `tungstenite-rs` Rust WebSocket library and provides `tokio` bindings and wrappers for it, so you
can use it with non-blocking/asynchronous `TcpStream`s from and couple it together with other crates from `tokio` stack.
