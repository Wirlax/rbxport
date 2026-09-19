# rbxport

rbxport is a GPL-2.0-or-later desktop application for managing a DJ library,
analysing local audio files, creating USB exports, and serving a local library
to supported players over a local network.

It is built with Tauri 2, Rust, and React for macOS and Windows.

## Safety

The application reads a local library by default. Writes require an explicit
action and are refused while a conflicting library process is running. Before
using write features, make an independent backup of your library.

## Development

```sh
pnpm install
pnpm dev
```

```sh
pnpm test
cargo test
```

### Logs

The app logs to stdout and to a daily file under
`~/Library/Application Support/rbxport/logs` (macOS) or
`%APPDATA%\rbxport\logs` (Windows), the last seven days kept. `LOG_LEVEL`
sets the level for the app's own crates — `error`, `warn`, `info`, `debug`
(the default) or `trace`, which adds LINK's packet-by-packet lines.
`RUST_LOG` replaces the whole filter when a per-crate mix is wanted
(`RUST_LOG=rbl_link=trace,rbxport=info`); `RBXPORT_LOG_DIR` moves the file.

## License

rbxport is licensed under GPL-2.0-or-later. See [LICENSE](LICENSE) and
[LICENSING.md](LICENSING.md).

## Trademark notice

All third-party product names are the property of their respective owners.
rbxport is an independent project and is not affiliated with, endorsed by, or
sponsored by any third-party product or trademark owner.
