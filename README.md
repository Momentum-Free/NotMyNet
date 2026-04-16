# NotMyNet
Wondering if your Internet connection is to blame or not? NotMyNet is the free, local-first solution for proving your connection is stable (or not)!

## Use Cases
- Check whether a known internet endpoint is reachable from a workstation over time.
- Monitor a local gateway, internal HTTP service, or router UI endpoint from a laptop on a LAN.
- Keep a rolling visual history of latency and outages while working from unstable networks.
- Export monitor definitions and history from one browser profile and import them elsewhere.

## Features
- Installable PWA
- Offline app shell
- Local monitor configuration
- HTTP(S) probe type
- Rolling window visualization
- Current state, streak, duration, average, worst
- JSON export/import
- Multiple monitors
- Basic settings

## Principles
- **Local-first**: all core logic, storage, and rendering happen in the browser.
- **Offline-first**: app shell and existing data must remain usable without internet access.
- **Transparent**: the UI must clearly distinguish browser limits from real target failures.
- **Deterministic**: metrics should be reproducible from stored samples.
- **Extensible**: probe types should use adapters so future browser-compatible checks can be added.

## Core User Stories
- As a user, I can install the app and reopen it offline.
- As a user, I can create a monitor that checks an HTTP(S) endpoint on an interval.
- As a user, I can see whether the target is currently online, degraded, or in outage.
- As a user, I can see the last N probe results in a colored rolling window.
- As a user, I can configure the rolling window size, defaulting to 60.
- As a user, I can see the current streak and how long the current state has lasted.
- As a user, I can see rolling average latency, rolling worst latency, and all-time worst latency.
- As a user, I can save all settings locally and keep them across reloads.
- As a user, I can export my monitors and settings as JSON.
- As a user, I can import a JSON file to restore monitors and settings.

## Tech Stack
- **Astro** for the app shell and UI composition.
- **TypeScript** throughout.
- **Bun** for package management, scripts, and local tooling.
- **Ky** for HTTP probe execution.
- **IndexedDB** for local persistence.
- **Service Worker** for offline app shell, installability and multi-tab coordination.
