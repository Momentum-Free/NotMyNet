import { h } from "preact";
import { useState, useEffect } from "preact/hooks";
import { Download, Upload, AlertCircle, ShieldCheck } from "lucide-preact";
import { listMonitors, listSamplesForMonitor, getSettings, saveSettings, saveMonitor, addSample } from "../lib/db";
import type { ExportBundle, MonitorConfig, ProbeSample } from "../lib/types";

function isValidMonitor(m: any): m is MonitorConfig {
    return (
        typeof m === "object" &&
        typeof m.id === "string" &&
        typeof m.name === "string" &&
        typeof m.url === "string" &&
        ["http"].includes(m.type) &&
        typeof m.intervalMs === "number" &&
        Array.isArray(m.expectedStatuses)
    );
}

function isValidSample(s: any): s is ProbeSample {
    return (
        typeof s === "object" &&
        typeof s.id === "string" &&
        typeof s.monitorId === "string" &&
        typeof s.startedAt === "number" &&
        ["online", "degraded", "outage"].includes(s.state)
    );
}

export function Settings() {
    const [importing, setImporting] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [persisted, setPersisted] = useState<boolean | null>(null);

    useEffect(() => {
        if (navigator.storage && navigator.storage.persisted) {
            navigator.storage.persisted().then(setPersisted);
        }
    }, []);

    const requestPersistence = async () => {
        if (navigator.storage && navigator.storage.persist) {
            const result = await navigator.storage.persist();
            setPersisted(result);
            if (result) {
                alert("Persistent storage granted by browser.");
            } else {
                alert("Persistent storage denied by browser. Data may be cleared if disk space is low.");
            }
        }
    };

    const handleExport = async () => {
        const monitors = await listMonitors();
        const settings = await getSettings();
        const allSamples = await Promise.all(monitors.map(m => listSamplesForMonitor(m.id)));
        const samples = allSamples.flat();

        const bundle: ExportBundle = {
            version: 1,
            exportedAt: Date.now(),
            settings,
            monitors,
            samples,
        };

        const blob = new Blob([JSON.stringify(bundle, null, 2)], { type: "application/json" });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = `notmynet-export-${new Date().toISOString().split('T')[0]}.json`;
        a.click();
        URL.revokeObjectURL(url);
    };

    const handleImport = async (e: Event) => {
        const file = (e.target as HTMLInputElement).files?.[0];
        if (!file) return;

        setImporting(true);
        setError(null);

        try {
            const text = await file.text();
            const bundle = JSON.parse(text) as ExportBundle;

            if (bundle.version !== 1) {
                throw new Error("Unsupported export version");
            }

            if (!Array.isArray(bundle.monitors) || !Array.isArray(bundle.samples)) {
                throw new Error("Invalid export bundle format");
            }

            // Validate Monitors
            for (const m of bundle.monitors) {
                if (!isValidMonitor(m)) throw new Error(`Invalid monitor object: ${m.name || m.id}`);
            }

            // Validate Samples
            for (const s of bundle.samples) {
                if (!isValidSample(s)) throw new Error(`Invalid sample object: ${s.id}`);
            }

            // Save settings
            if (bundle.settings) {
                await saveSettings(bundle.settings);
            }

            // Save monitors and samples
            for (const monitor of bundle.monitors) {
                await saveMonitor(monitor);
            }
            for (const sample of bundle.samples) {
                await addSample(sample);
            }

            // Notify worker
            try {
                const worker = new SharedWorker(new URL("../lib/worker.ts", import.meta.url), { type: "module" });
                worker.port.postMessage({ type: "refresh" });
            } catch (e) {}

            alert("Import successful!");
            window.location.reload();
        } catch (err: any) {
            setError(err.message || "Failed to import file");
        } finally {
            setImporting(false);
        }
    };

    return (
        <div class="max-w-2xl mx-auto space-y-8">
            <section class="bg-slate-900 border border-slate-800 rounded-xl p-6">
                <h3 class="text-lg font-semibold text-slate-100 mb-4 flex items-center gap-2">
                    <ShieldCheck class="w-5 h-5 text-emerald-400" />
                    Storage Durability
                </h3>
                <p class="text-slate-400 text-sm mb-6">
                    By default, the browser may clear local data if the device runs low on disk space. You can request persistent storage to prevent this.
                </p>

                {persisted === true ? (
                    <div class="p-3 bg-emerald-900/20 border border-emerald-900/30 rounded-lg flex items-center gap-2 text-emerald-400 text-sm">
                        <ShieldCheck class="w-4 h-4" />
                        Storage is persistent. Your data is protected from automatic deletion.
                    </div>
                ) : (
                    <button
                        onClick={requestPersistence}
                        class="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 rounded-lg transition flex items-center gap-2"
                    >
                        Request Persistent Storage
                    </button>
                )}
            </section>

            <section class="bg-slate-900 border border-slate-800 rounded-xl p-6">
                <h3 class="text-lg font-semibold text-slate-100 mb-4 flex items-center gap-2">
                    <Download class="w-5 h-5 text-blue-400" />
                    Data Portability
                </h3>
                <p class="text-slate-400 text-sm mb-6">
                    Export your configurations and monitoring history to a JSON file, or restore them from a previous backup.
                </p>

                <div class="flex flex-wrap gap-4">
                    <button
                        onClick={handleExport}
                        class="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 rounded-lg transition flex items-center gap-2"
                    >
                        <Download class="w-4 h-4" />
                        Export Data
                    </button>

                    <label class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg transition flex items-center gap-2 cursor-pointer">
                        <Upload class="w-4 h-4" />
                        {importing ? "Importing..." : "Import Data"}
                        <input type="file" accept=".json" onChange={handleImport} class="hidden" disabled={importing} />
                    </label>
                </div>

                {error && (
                    <div class="mt-4 p-3 bg-red-900/20 border border-red-900/30 rounded-lg flex items-center gap-2 text-red-400 text-sm">
                        <AlertCircle class="w-4 h-4" />
                        {error}
                    </div>
                )}
            </section>

            <section class="bg-slate-900 border border-slate-800 rounded-xl p-6">
                <h3 class="text-lg font-semibold text-slate-100 mb-4 flex items-center gap-2">
                    <AlertCircle class="w-5 h-5 text-orange-400" />
                    Monitoring Limitations
                </h3>
                <div class="space-y-3 text-sm text-slate-400">
                    <p>• Monitoring is performed entirely within your browser.</p>
                    <p>• Target compatibility depends on browser CORS (Cross-Origin Resource Sharing) constraints.</p>
                    <p>• Background execution may be heavily throttled or paused by the browser when the tab is not active.</p>
                    <p>• This is not a network-layer ICMP ping; it measures application-layer HTTP reachability.</p>
                </div>
            </section>
        </div>
    );
}
