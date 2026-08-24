# JRX Network Observatory

A local-first network intelligence application.

## Vision

Understand your network environment.

JRX Observatory detects:

- network connection type
- visibility level
- available device intelligence
- traffic capabilities

## Principles

- Local processing first
- No packet content collection
- No credential collection
- No hidden monitoring
- Explain what is visible and what is not

## Platforms

Initial:
- macOS
- Windows

Future:
- Android
- iOS

## Running JRX

Requires the Rust toolchain on `PATH`:

```
. "$HOME/.cargo/env"
```

**Development** — starts Vite and the app together, with hot reload:

```
cd app && npm install && npm run dev:app
```

**Standalone build** — produces an app with the frontend embedded:

```
cd app && npm run build:app
```

### Do not run the binary from `cargo` directly

`cargo run -p jrx-app` and `./target/{debug,release}/jrx-app` produce **a blank
white window**, in both debug and release.

`tauri.conf.json` sets `build.devUrl`, and `tauri::generate_context!()` only
embeds `app/dist` when the build is driven by the Tauri CLI. A bare `cargo`
build — at any optimisation level — bakes in `http://localhost:1420` instead,
so the window renders only while a Vite dev server happens to be running and
is empty otherwise. The cargo profile makes no difference; only the CLI does.

Use `npm run dev:app` or `npm run build:app`.
