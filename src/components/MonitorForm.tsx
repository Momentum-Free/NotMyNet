import { h } from "preact";
import { useState, useEffect } from "preact/hooks";
import type { MonitorConfig } from "../lib/types";
import { saveMonitor, listMonitors } from "../lib/db";
import { ChevronLeft, Save, Globe } from "lucide-preact";
import { CLOUDFLARE_PRESET } from "../lib/probes/http";

interface Props {
    id?: string;
}

export function MonitorForm({ id }: Props) {
    const [config, setConfig] = useState<Partial<MonitorConfig>>({
        name: "",
        url: "https://",
        type: "http",
        method: "GET",
        intervalMs: 10000,
        timeoutMs: 5000,
        windowSize: 60,
        expectedStatuses: [200],
        degradedMs: 500,
        enabled: true,
    });

    const [loading, setLoading] = useState(!!id);

    useEffect(() => {
        if (id) {
            listMonitors().then(monitors => {
                const existing = monitors.find(m => m.id === id);
                if (existing) {
                    setConfig(existing);
                }
                setLoading(false);
            });
        }
    }, [id]);

    const handleSubmit = async (e: Event) => {
        e.preventDefault();
        const now = Date.now();
        const finalConfig: MonitorConfig = {
            ...(config as MonitorConfig),
            id: id || crypto.randomUUID(),
            createdAt: (config as any).createdAt || now,
            updatedAt: now,
        };
        await saveMonitor(finalConfig);

        // Notify worker
        const sw = await window.navigator.serviceWorker?.ready; // This is for SW, but we use Shared Worker
        // Shared Worker needs a way to be notified.
        // We'll handle this by sending a message to the Shared Worker.
        try {
            const worker = new SharedWorker(new URL("../lib/worker.ts", import.meta.url), { type: "module" });
            worker.port.postMessage({ type: "refresh" });
        } catch (e) {
            console.warn("SharedWorker not supported, relying on worker auto-refresh or reload");
        }

        window.location.href = "/";
    };

    const loadPreset = () => {
        setConfig({
            ...config,
            ...CLOUDFLARE_PRESET,
        });
    };

    if (loading) return <div class="p-8 text-center text-slate-400">Loading config...</div>;

    return (
        <form onSubmit={handleSubmit} class="space-y-6 max-w-2xl mx-auto">
            <div class="flex items-center justify-between">
                <a href="/" class="text-slate-400 hover:text-white flex items-center gap-1 text-sm">
                    <ChevronLeft class="w-4 h-4" />
                    Back to Dashboard
                </a>
                {!id && (
                    <button
                        type="button"
                        onClick={loadPreset}
                        class="text-xs bg-blue-600/10 text-blue-400 border border-blue-600/20 px-3 py-1.5 rounded-lg hover:bg-blue-600/20 transition flex items-center gap-2"
                    >
                        <Globe class="w-3.5 h-3.5" />
                        Load Internet Preset
                    </button>
                )}
            </div>

            <div class="bg-slate-900 border border-slate-800 rounded-xl p-6 space-y-4">
                <div class="grid gap-2">
                    <label class="text-xs font-bold text-slate-500 uppercase">Monitor Name</label>
                    <input
                        required
                        value={config.name}
                        onInput={e => setConfig({...config, name: (e.target as HTMLInputElement).value})}
                        placeholder="My Website"
                        class="bg-slate-950 border border-slate-800 rounded-lg px-4 py-2 text-slate-200 focus:outline-none focus:border-blue-500"
                    />
                </div>

                <div class="grid gap-2">
                    <label class="text-xs font-bold text-slate-500 uppercase">Target URL</label>
                    <input
                        required
                        type="url"
                        value={config.url}
                        onInput={e => setConfig({...config, url: (e.target as HTMLInputElement).value})}
                        placeholder="https://example.com"
                        class="bg-slate-950 border border-slate-800 rounded-lg px-4 py-2 text-slate-200 font-mono focus:outline-none focus:border-blue-500"
                    />
                </div>

                <div class="grid grid-cols-2 gap-4">
                    <div class="grid gap-2">
                        <label class="text-xs font-bold text-slate-500 uppercase">Method</label>
                        <select
                            value={config.method}
                            onChange={e => setConfig({...config, method: (e.target as HTMLSelectElement).value as any})}
                            class="bg-slate-950 border border-slate-800 rounded-lg px-4 py-2 text-slate-200 focus:outline-none focus:border-blue-500"
                        >
                            <option value="GET">GET</option>
                            <option value="HEAD">HEAD</option>
                        </select>
                    </div>
                    <div class="grid gap-2">
                        <label class="text-xs font-bold text-slate-500 uppercase">Expected Status</label>
                        <input
                            value={config.expectedStatuses?.join(", ")}
                            onInput={e => setConfig({...config, expectedStatuses: (e.target as HTMLInputElement).value.split(",").map(s => parseInt(s.trim())).filter(s => !isNaN(s))})}
                            placeholder="200, 204"
                            class="bg-slate-950 border border-slate-800 rounded-lg px-4 py-2 text-slate-200 focus:outline-none focus:border-blue-500"
                        />
                    </div>
                </div>

                <div class="grid grid-cols-3 gap-4">
                    <div class="grid gap-2">
                        <label class="text-xs font-bold text-slate-500 uppercase">Interval (ms)</label>
                        <input
                            type="number"
                            value={config.intervalMs}
                            onInput={e => setConfig({...config, intervalMs: parseInt((e.target as HTMLInputElement).value)})}
                            class="bg-slate-950 border border-slate-800 rounded-lg px-4 py-2 text-slate-200 focus:outline-none focus:border-blue-500"
                        />
                    </div>
                    <div class="grid gap-2">
                        <label class="text-xs font-bold text-slate-500 uppercase">Timeout (ms)</label>
                        <input
                            type="number"
                            value={config.timeoutMs}
                            onInput={e => setConfig({...config, timeoutMs: parseInt((e.target as HTMLInputElement).value)})}
                            class="bg-slate-950 border border-slate-800 rounded-lg px-4 py-2 text-slate-200 focus:outline-none focus:border-blue-500"
                        />
                    </div>
                    <div class="grid gap-2">
                        <label class="text-xs font-bold text-slate-500 uppercase">Degraded (ms)</label>
                        <input
                            type="number"
                            value={config.degradedMs}
                            onInput={e => setConfig({...config, degradedMs: parseInt((e.target as HTMLInputElement).value)})}
                            class="bg-slate-950 border border-slate-800 rounded-lg px-4 py-2 text-slate-200 focus:outline-none focus:border-blue-500"
                        />
                    </div>
                </div>

                <div class="grid gap-2">
                    <label class="text-xs font-bold text-slate-500 uppercase">Rolling Window Size</label>
                    <input
                        type="number"
                        value={config.windowSize}
                        onInput={e => setConfig({...config, windowSize: parseInt((e.target as HTMLInputElement).value)})}
                        class="bg-slate-950 border border-slate-800 rounded-lg px-4 py-2 text-slate-200 focus:outline-none focus:border-blue-500"
                    />
                </div>

                <div class="flex items-center gap-2 pt-2">
                    <input
                        type="checkbox"
                        id="enabled"
                        checked={config.enabled}
                        onChange={e => setConfig({...config, enabled: (e.target as HTMLInputElement).checked})}
                        class="w-4 h-4 rounded border-slate-800 bg-slate-950 text-blue-600 focus:ring-blue-500 focus:ring-offset-slate-900"
                    />
                    <label htmlFor="enabled" class="text-sm font-medium text-slate-300">Enabled</label>
                </div>
            </div>

            <button
                type="submit"
                class="w-full bg-blue-600 hover:bg-blue-500 text-white font-bold py-3 rounded-xl transition flex items-center justify-center gap-2 shadow-lg shadow-blue-600/20"
            >
                <Save class="w-5 h-5" />
                {id ? "Update Monitor" : "Create Monitor"}
            </button>
        </form>
    );
}
