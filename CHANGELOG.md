# 0.3.0

- Change (breaking): Split topics into two types. `Path` is a path without a protocol (`a/b/c`, or `""` for the root). `Topic` is a protocol plus a `Path`, with string form `<protocol>:<path>` (e.g. `f32:a/b/c`). Both are exported from the prelude.
  - `subscribe`, `subscribe_into`, `publish`, `serve` and `request` take a `Path`. Subscriptions yield the `Path` a message was published at instead of the prefixed topic string.
  - `subscribe_bytes`, `subscribe_bytes_into`, `publish_bytes` and `request_bytes` take a `Topic`, and `PubMsg::topic` is a `Topic`. The protocol may be the wildcard `*` to match all protocols.
  - The `t2` CLI takes topics as `<protocol>:<path>`, e.g. `t2 pub f32:a/b 1.5` or `t2 ls '*:**'`.
  - `BusError::ServiceNotFound` carries a `Topic`.
  - Wire format change: topics are sent as a `(protocol, path)` pair and validated on decode. Clients and servers must both be upgraded.
- Change: The double wildcard `**` now matches zero segments as well as one or more, e.g. `a/**` matches `a` and `a/**/b` matches `a/b`. A subscribed `**` also now correctly matches a published `*` followed by further segments.
- Fix: `t2` CLI errors (e.g. invalid arguments) are now printed. Previously the default log filter hid the binary's own log messages.
- Change: Migrate diagnostic logging from `println!` to the `log` crate. The `t2` binary initializes `env_logger` with a default filter of `t2_bus=info`; protocol message tracing is available via `RUST_LOG=t2_bus=trace`.

# 0.2.0

- Add: impl `futures::Stream` for `Subscription` and `RequestSubscription`.