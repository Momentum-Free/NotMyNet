import { listMonitors, addSample, pruneOldSamples, getSettings } from "./db";
import { runHttpProbe } from "./probes/http";
import type { MonitorConfig } from "./types";

const activeMonitors = new Map<string, any>(); // monitorId -> timeoutId
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

        const timeoutId = setTimeout(run, monitor.intervalMs);
        activeMonitors.set(monitor.id, timeoutId);
    };

    // Initial run with a small random delay to avoid thundering herd if multiple monitors
    const initialDelay = Math.random() * 1000;
    const timeoutId = setTimeout(run, initialDelay);
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
    try {
        const settings = await getSettings();
        await pruneOldSamples(settings.retentionDays);
    } catch (e) {
        console.error("Pruning failed", e);
    }
    // Prune once a day
    setTimeout(startPruning, 24 * 60 * 60 * 1000);
}

// Shared Worker boilerplate
const ctx = self as any;
const ports = new Set<MessagePort>();

ctx.onconnect = (e: any) => {
    const port = e.ports[0];
    ports.add(port);

    port.onmessage = async (msg: any) => {
        if (msg.data.type === "refresh") {
            await refreshMonitors();
            // Broadcast to all other tabs that they should refresh their UI
            broadcast.postMessage({ type: "refresh" });
        }
        if (msg.data.type === "ping") {
            port.postMessage({ type: "pong" });
        }
    };

    port.onmessageerror = () => {
        ports.delete(port);
    };

    port.start();
};

// Initialization
refreshMonitors();
startPruning();
