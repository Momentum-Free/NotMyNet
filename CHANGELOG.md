# Changelog

## Windows (unreleased)

- The normal Ping Strip follows the physical display edge when a borderless fullscreen window is active on that display; otherwise it uses the work-area edge.
- The main window sizes itself to fit the saved probe history at launch, up to the primary display’s work area. Probe cards stretch across wider windows.

## Windows v0.1.0

First public Windows x64 release of the native NotMyNet monitor.

- Monitor an IPv4 or IPv6 address or DNS hostname with ICMP, independently of the web app’s HTTP(S) checks.
- Inspect individual replies and RTTs in the rolling history and track latency, loss, streak and outage duration.
- See Windows-reported link rates and passive Rx/Tx peaks for the route-selected adapter. These peaks include other traffic on that adapter; the app does not run a speed test.
- Optionally show a click-through Ping Strip on a chosen display edge or behind a supported transparent taskbar. Exclusive fullscreen is not guaranteed.
- Configure timing, history and appearance in the separate Settings window.

Download the `NotMyNet-Windows-x64-0.1.0.zip` asset, extract it, and open `notmynet-windows.exe`. The ZIP targets Windows 11 x64. No installer or administrator access is required. This is an early, unsigned release; Windows SmartScreen may warn. No macOS build is included.
