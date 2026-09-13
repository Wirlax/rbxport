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

## License

rbxport is licensed under GPL-2.0-or-later. See [LICENSE](LICENSE) and
[LICENSING.md](LICENSING.md).

## Trademark notice

All third-party product names are the property of their respective owners.
rbxport is an independent project and is not affiliated with, endorsed by, or
sponsored by any third-party product or trademark owner.
