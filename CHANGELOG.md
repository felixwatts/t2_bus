# 0.3.0

- Change: Migrate diagnostic logging from `println!` to the `log` crate. The `t2` binary initializes `env_logger` with a default filter of `t2_bus=info`; protocol message tracing is available via `RUST_LOG=t2_bus=trace`.

# 0.2.0

- Add: impl `futures::Stream` for `Subscription` and `RequestSubscription`.