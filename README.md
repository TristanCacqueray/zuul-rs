# A rust client for zuul-ci.org

[![crates.io](https://img.shields.io/crates/v/zuul.svg)](https://crates.io/crates/zuul)

Use this library to query and decode zuul-web API.

## Features

- serde decoder for API endpoints.
- reqwest client wrapper.
- tokio async-stream for builds result.

## Usage

Please read the [documentation here](https://docs.rs/zuul/).
Additional learning resources: [rust-cookbook](https://rust-lang-nursery.github.io/rust-cookbook/).

How to use with Cargo:

```toml
[dependencies]
zuul = "0.1.0"
```

How to use in your crate:

```rust
use zuul;
```

How to run the zuul-builds stream utility:

```ShellSession
$ cargo run --example zuul-build -- --url https://zuul.opendev.org/api/tenant/openstack
```

If you experience any difficulties, please don't hesistate to raise an issue.
