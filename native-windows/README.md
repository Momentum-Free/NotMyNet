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

In the app, use `127.0.0.1`, `::1`, then `localhost`. Exercise Start/Stop, immediately start a different target, and confirm no old results reappear. Open Settings twice and check that it focuses the same separate window. Check Apply/Cancel, narrow/wide layout, and System against a Windows app-theme change; explicit Light and Dark should override it and persist after relaunch. Do not claim clean-Windows compatibility from a machine with build tools. Controlled timeout/overlap, 50 ms cadence in native execution, and clean standard-user launch need separate Windows evidence before handing out a build.

## Probe and history behavior

`Target` accepts a single unicast IP or DNS hostname. The monitor resolves a hostname once per Start and uses one eligible address from that result until stopped. It does not check HTTPS or general internet health. IPv6 link-local addresses requiring an interface scope and non-unicast targets are not supported.

The requested interval sets a fixed monotonic grid of slots (default 1,000 ms); each initiated probe has a separate timeout (default 1,000 ms). An outstanding attempt does not delay the next slot. Overdue/saturated slots can be skipped without replay, and pending and skipped slots are distinct from failed replies. Scheduling jitter is possible; a 50 ms interval is a target, not a timing guarantee. A 60-slot history at the default interval represents a nominal minute, including unsampled slots, irrespective of reply times. Min, Avg and Worst RTT use successful replies only. `Error/Timeout` identifies a failed check; the detail distinguishes startup/local errors from probe failures. Only probe failures contribute to loss and start the elapsed outage timer. Neither proves service downtime.

A maximum timeout of 5 seconds and a minimum interval of 50 ms bound native work. Stop lets in-flight native attempts finish off the UI thread and discards their output. Settings are stored under `%LOCALAPPDATA%\NotMyNet\settings.txt`; probe settings take effect on the next Start, and appearance applies immediately. No monitoring history is written to disk.
