import { listMonitors, addSample, pruneOldSamples, getSettings } from "./db";
import { runHttpProbe } from "./probes/http";
import type { MonitorConfig, ProbeSample } from "./types";

const activeMonitors = new Map<string, number>(); // monitorId -> timeoutId
const broadcast = new BroadcastChannel("notmynet_updates");

async function scheduleProbe(monitor: MonitorConfig) {
    if (!monitor.enabled) return;

    // Clear existing if any
    if (activeMonitors.has(monitor.id)) {
        clearTimeout(activeMonitors.get(monitor.id));
    }

    const run = async () => {
        if (!monitor.enabled) return;

        const sample = await runHttpProbe(monitor);
        await addSample(sample);

        broadcast.postMessage({ type: "sample", sample });

        const timeoutId = setTimeout(run, monitor.intervalMs) as unknown as number;
        activeMonitors.set(monitor.id, timeoutId);
    };

    // Initial run with a small random delay to avoid thundering herd if multiple monitors
    const initialDelay = Math.random() * 1000;
    const timeoutId = setTimeout(run, initialDelay) as unknown as number;
    activeMonitors.set(monitor.id, timeoutId);
}

async function refreshMonitors() {
    const monitors = await listMonitors();

    // Stop monitors that are no longer in DB or disabled
    const monitorIds = new Set(monitors.map(m => m.id));
    for (const [id, timeoutId] of activeMonitors.entries()) {
        if (!monitorIds.has(id)) {
            clearTimeout(timeoutId);
            activeMonitors.delete(id);
        }
    }

    // Start/Update monitors
    for (const monitor of monitors) {
        if (monitor.enabled) {
            // If already running, we might want to update it if config changed
            // For simplicity, let's just reschedule everything on refresh for now
            // or we could check if interval changed.
            scheduleProbe(monitor);
        } else {
            if (activeMonitors.has(monitor.id)) {
                clearTimeout(activeMonitors.get(monitor.id));
                activeMonitors.delete(monitor.id);
            }
        }
    }
}

// Pruning loop
async function startPruning() {
    const settings = await getSettings();
    await pruneOldSamples(settings.retentionDays);
    // Prune once a day
    setTimeout(startPruning, 24 * 60 * 60 * 1000);
}

// Shared Worker boilerplate
const self = globalThis as unknown as SharedWorkerGlobalScope;

self.onconnect = (e) => {
    const port = e.ports[0];

    port.onmessage = async (msg) => {
        if (msg.data.type === "refresh") {
            await refreshMonitors();
        }
        if (msg.data.type === "ping") {
            port.postMessage({ type: "pong" });
        }
    };

    port.start();
};

// Initialization
refreshMonitors();
startPruning();
