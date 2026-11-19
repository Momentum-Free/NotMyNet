# NotMyNet for Windows — development candidate

A GPUI Kit 0.10.0 native desktop monitor. This is separate from the browser app and the CLI scripts. No service, administrator privileges, raw sockets or shell commands are used by the app.

## Build and test

Use Windows 11 x64, the `x86_64-pc-windows-msvc` Rust toolchain (development pin: 1.100.0), MSVC C++ Build Tools, Windows SDK and CMake. From a native x64 Developer PowerShell, with the approved local Rust toolchain on that session’s PATH:

```powershell
cd native-windows
cargo test --locked --target x86_64-pc-windows-msvc -- --nocapture
cargo build --locked --release --target x86_64-pc-windows-msvc
.\target\x86_64-pc-windows-msvc\release\notmynet-windows.exe
```

The Windows tests use only IP loopback and `localhost` after confirming it resolves to loopback. Non-Windows `cargo test --locked` exercises the portable history, duration and settings logic, not the UI or Windows ICMP. Cross-target `cargo check` is compilation evidence, not a Windows run.

In the app, use `127.0.0.1`, `::1`, then `localhost`. Hover a reply tile and an empty tile: expect the individual RTT (including `0 ms`) or an explicit state. Check that the successful-probe streak continues beyond the display count and resets on a new run. For loopback, Link speed should say unavailable. Where authorized, use a LAN target routed over Ethernet or Wi-Fi, compare the indicated type and Rx/Tx rates to the Windows adapter's reported link rates, and verify there is no stale adapter result after switching back to loopback. Exercise Start/Stop, immediately start a different target, and confirm no old results reappear. Open Settings twice and check that it focuses the same separate window. Check Apply/Cancel, narrow/wide layout, and System against a Windows app-theme change; explicit Light and Dark should override it and persist after relaunch. Do not claim clean-Windows compatibility from a machine with build tools. Controlled timeout/overlap, 50 ms cadence in native execution, and clean standard-user launch need separate Windows evidence before handing out a build.

## Probe and history behavior

`Target` accepts a single unicast IP or DNS hostname. The monitor resolves a hostname once per Start and uses one eligible address from that result until stopped. It does not check HTTPS or general internet health. IPv6 link-local addresses requiring an interface scope and non-unicast targets are not supported.

The requested interval sets a fixed monotonic grid of slots (default 1,000 ms); each initiated probe has a separate timeout (default 1,000 ms). An outstanding attempt does not delay the next slot. Overdue/saturated slots can be skipped without replay, and pending and skipped slots are distinct from failed replies. Scheduling jitter is possible; a 50 ms interval is a target, not a timing guarantee. A 60-slot history at the default interval represents a nominal minute, including unsampled slots, irrespective of reply times. Min, Avg and Worst RTT use successful replies only. Hover a tile for its RTT or state. Streak counts consecutive successful slots across the session; failures and skipped slots break the streak, and pending slots are not successes. Link speed shows Windows' reported receive/transmit link rates for the physical interface selected by the route to the target, not measured throughput. It is unavailable on loopback or when there is no usable physical link data. Max. Speed captured shows the highest sampled Rx and Tx throughput independently, calculated from the selected adapter’s Windows byte counters about every two seconds. It includes traffic from all apps on that adapter, not just the target, and does not generate test traffic. It needs two fresh readings, resets on Start or a route change, and retains the last valid displayed peak when stopped. `Error/Timeout` identifies a failed check; the detail distinguishes startup/local errors from probe failures. Only probe failures contribute to loss and start the elapsed outage timer. Neither proves service downtime.

## Screen-edge overlay

Settings can turn the overlay on (off by default), select a display and edge, and set its thickness in physical pixels. It uses the same probe history and response colors as the monitor, from left to right or top to bottom. The borderless, click-through strip remains while monitoring with the main window minimized, and hides on Stop. It normally spans the selected display’s work-area edge without covering the taskbar or reserving screen space.

**Behind transparent taskbar** uses the whole taskbar area instead of the configured thickness. It hides with an error if Windows transparency is disabled or cannot be confirmed, or if the app cannot verify a visible taskbar and safe ordering on the selected edge. The app does not change the taskbar, transparency settings, or fullscreen behavior. Visibility through the taskbar and over games is not guaranteed (especially in exclusive fullscreen).

## Runtime limits

A maximum timeout of 5 seconds and a minimum interval of 50 ms bound native work. Stop lets in-flight native attempts finish off the UI thread and discards their output. Settings are stored under `%LOCALAPPDATA%\NotMyNet\settings.txt`; probe settings take effect on the next Start, and appearance applies immediately. No monitoring history is written to disk.
