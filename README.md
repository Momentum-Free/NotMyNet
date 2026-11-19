# NotMyNet

Is it your internet connection, or the thing you're trying to reach? NotMyNet is a free, local-first reachability monitor.

| App | What it checks | Get it |
| --- | --- | --- |
| Web app | HTTP(S) endpoints reachable from your browser | [Open not-my.net](https://not-my.net/) |
| Windows desktop | ICMP ping to one IPv4 or IPv6 address or hostname | [Latest Windows release](https://github.com/Momentum-Free/NotMyNet/releases/latest) |

The installable web app keeps monitor definitions and history in the browser, supports JSON import/export, and can reopen offline. Browser restrictions still apply to its checks. The native app shows per-ping latency and loss, route-selected link speed and sampled adapter throughput, and an optional click-through Ping Strip. A missed ping does not mean an HTTP service is down.

### Windows download

Download the `NotMyNet-Windows-x64-*.zip` asset (not the source archive), extract it, and open `notmynet-windows.exe`. The early desktop release targets Windows 11 x64, requires no installer or elevation, and is unsigned, so SmartScreen may warn. No macOS desktop build is available. See the [Windows docs](native-windows/README.md) for settings and known limits.

## Develop

This is a Bun monorepo: [`apps/web`](apps/web) is the Astro + Preact workspace; [`native-windows`](native-windows) is a Rust/Cargo package with its own lockfile. Building the web app does not require a Rust toolchain.

```sh
bun install --frozen-lockfile
bun run dev
bun run build   # static site and PWA assets in dist/
bun run preview
```

Cloudflare Pages builds from the repository root and publishes `dist/`. For native builds, see the [Windows toolchain and test instructions](native-windows/README.md#build-and-test).

## Releases

The desktop app uses SemVer in `native-windows/Cargo.toml`, tracked in the [changelog](CHANGELOG.md) and tagged `windows-v<version>`. The web app deploys independently from `main`.

To release Windows: update `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md`; dispatch the **Windows release build** GitHub workflow for the candidate commit. After tests and the packaged app's launch check pass, download the workflow artifact, verify `SHA256SUMS.txt`, and publish its ZIP and checksum to a GitHub Release tagged at that exact commit. Use the matching changelog entry for notes. Keep the latest public release pointing to a Windows build while the website uses `/releases/latest`.

[MIT License](LICENSE)
