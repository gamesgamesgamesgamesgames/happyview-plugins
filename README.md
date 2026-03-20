# HappyView Plugins

WASM plugins for [HappyView](https://github.com/gamesgamesgamesgamesgames/happyview) that provide auth and data sync from external platforms.

## Available Plugins

| Plugin        | Platform            | Auth Type | Required Secrets             |
| ------------- | ------------------- | --------- | ---------------------------- |
| `steam`       | Steam               | OpenID    | `API_KEY`                    |
| `xbox`        | Xbox/Microsoft      | OAuth2    | `CLIENT_ID`, `CLIENT_SECRET` |
| `gog`         | GOG Galaxy          | OAuth2    | `CLIENT_ID`, `CLIENT_SECRET` |
| `playstation` | PlayStation Network | OAuth2    | None (public client)         |
| `itch`        | itch.io             | OAuth2    | `CLIENT_ID`, `CLIENT_SECRET` |

## Installation

Download the `.wasm` files from the [latest release](https://github.com/gamesgamesgamesgamesgames/happyview-plugins/releases/latest) and place them in your HappyView plugins directory.

## Building from Source

Requirements:

- Rust with `wasm32-unknown-unknown` target

```bash
# Add WASM target if needed
rustup target add wasm32-unknown-unknown

# Build all plugins
cargo build --release --target wasm32-unknown-unknown

# Plugins will be in target/wasm32-unknown-unknown/release/*.wasm
```

## Configuration

Each plugin requires environment variables in HappyView with the prefix `PLUGIN_{PLUGIN_ID}_`:

```bash
# Steam
PLUGIN_STEAM_API_KEY=your_steam_api_key

# Xbox
PLUGIN_XBOX_CLIENT_ID=your_azure_client_id
PLUGIN_XBOX_CLIENT_SECRET=your_azure_client_secret

# GOG (requires developer access)
PLUGIN_GOG_CLIENT_ID=your_gog_client_id
PLUGIN_GOG_CLIENT_SECRET=your_gog_client_secret

# itch.io
PLUGIN_ITCH_CLIENT_ID=your_itch_client_id
PLUGIN_ITCH_CLIENT_SECRET=your_itch_client_secret
```
