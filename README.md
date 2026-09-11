# HappyView Plugins

WASM plugins for [HappyView](https://github.com/gamesgamesgamesgamesgames/happyview) that provide auth and data sync from external platforms.

## Available Plugins

| Plugin      | Platform  | Auth Type | Syncs                    |
| ----------- | --------- | --------- | ------------------------ |
| `steam`     | Steam     | OpenID    | Games library            |
| `xbox`      | Xbox      | OAuth2    | Xbox games, achievements |
| `microsoft` | Microsoft | OAuth2    | Account linking          |
| `itch`      | itch.io   | OAuth2    | Games library            |

## Library Plugins

Library plugins expose functions to HappyView scripts (`require("<namespace>")` in Lua). They declare the capabilities they need in `manifest.json`; the HappyView loader refuses a plugin whose WASM imports need more than it declares.

| Plugin | Namespace | Capabilities                    | Provides                                                   |
| ------ | --------- | ------------------------------- | ---------------------------------------------------------- |
| `http` | `http`    | `network:request:unrestricted`  | `get`, `post`, `put`, `patch`, `delete`, `head`            |

Requires HappyView v3 (plugin API `api_version` `"2"`).

## Writing a plugin with the SDK

`happyview-plugin-sdk` owns everything between a plugin and the host: the guest allocator, the packed-`i64` calling convention, the JSON envelope, and the `env` host imports. A plugin crate needs no `extern "C"` block, no raw pointers, and no `#[global_allocator]` — `library_plugin!` emits all of it, in the plugin crate, where the wasm linker reliably keeps the exports.

The SDK lives in the HappyView repo, not this one, at `crates/happyview-plugin-sdk`. Until it is published to crates.io, this workspace depends on a sibling checkout at `../../atproto/happyview` (adjust the path in `Cargo.toml` if your checkout differs):

```toml
# root Cargo.toml
[workspace.dependencies]
happyview-plugin-sdk = { path = "../../atproto/happyview/crates/happyview-plugin-sdk" }
```

```toml
# plugins/<name>/Cargo.toml
[lib]
crate-type = ["cdylib"]

[dependencies]
happyview-plugin-sdk.workspace = true
```

Describe the plugin, its API surface, and how to dispatch a call. That is the whole contract:

```rust
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use happyview_plugin_sdk::host::{self, HttpRequest};
use happyview_plugin_sdk::{
    library_plugin, ApiExport, ApiSurface, CallContext, PluginError, PluginInfo, Value,
};

library_plugin! {
    info: PluginInfo::new("http", "HTTP Client", "1.0.0"),
    surface: surface,
    call: dispatch,
}

fn surface() -> ApiSurface {
    ApiSurface::new("http")
        .describe("Outbound HTTP requests")
        .export(
            ApiExport::function("get")
                .describe("Send a GET request")
                .param("url", "string", "Target URL"),
        )
}

fn dispatch(function: &str, args: &[Value], _ctx: &CallContext) -> Result<Value, PluginError> {
    match function {
        "get" => {
            let url = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| PluginError::bad_input("url is required"))?;
            let response = host::http_request(&HttpRequest::new("GET", url))?;
            Ok(happyview_plugin_sdk::json!({"status": response.status, "body": response.body}))
        }
        other => Err(PluginError::unknown_function(other)),
    }
}
```

`plugins/http` is this, in full, in about a hundred lines.

The macro emits `alloc`, `dealloc`, `plugin_info`, `get_api_surface` and `call`, plus the bump allocator and the wasm `#[panic_handler]`. Pass `heap = <bytes>` as the first field to size the guest heap; it defaults to 512 KiB. A plugin that exports something other than the library ABI calls `export_abi!()` on its own and writes its exports by hand.

### Host functions and capabilities

`happyview_plugin_sdk::host` wraps all eleven host imports. Each wrapper's doc comment names the capability the plugin's `manifest.json` must declare, and the HappyView loader refuses a plugin whose wasm imports need more than it declared. The linker drops an import along with the code that would have called it, so an unused wrapper costs nothing — `happyview-plugin-sdk`'s own `tests/exports.rs`, in the HappyView repo, pins that against its own `http`-shaped fixture (not against this repo's `plugins/http` build).

| Wrapper | Import | Capability |
| ------- | ------ | ---------- |
| `log` / `debug` / `info` / `warn` / `error` | `host_log` | none |
| `get_secret` | `host_get_secret` | `secrets:read` |
| `http_request` | `host_http_request` | `network:request` or `network:request:unrestricted` |
| `kv_get` | `host_kv_get` | `kv:read` |
| `kv_set` / `kv_delete` | `host_kv_set` / `host_kv_delete` | `kv:write` |
| `lookup_record` | `host_lookup_record` | `records:read` |
| `call_library` / `library_surface` | `host_call_library` / `host_get_api_surface` | `library:call` |
| `db_query` | `host_db_query` | `database:read` or `database:write` |
| `db_execute` | `host_db_execute` | `database:write` |

Native builds compile the whole SDK, so a plugin's own logic is testable with `cargo test`; off wasm32 every host wrapper returns `HostError::NotWasm` rather than calling anything. The SDK's own test suite runs in the HappyView repo's CI, not here.

```bash
cargo build --release --target wasm32-unknown-unknown -p http-plugin
cargo clippy --target wasm32-unknown-unknown -p http-plugin -- -D warnings
```

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

# Xbox (Azure AD app with Xbox Live scopes)
PLUGIN_XBOX_CLIENT_ID=your_azure_client_id
PLUGIN_XBOX_CLIENT_SECRET=your_azure_client_secret

# Microsoft (can use same Azure app as Xbox)
PLUGIN_MICROSOFT_CLIENT_ID=your_azure_client_id
PLUGIN_MICROSOFT_CLIENT_SECRET=your_azure_client_secret

# itch.io
PLUGIN_ITCH_CLIENT_ID=your_itch_client_id
PLUGIN_ITCH_CLIENT_SECRET=your_itch_client_secret
```
