import { h } from "preact";
import { useState, useEffect, useMemo } from "preact/hooks";
import type { MonitorViewModel, ProbeSample, ProbeState } from "../lib/types";
import { computeSummary, getColorForState, formatDuration } from "../lib/utils";
import { Activity, Clock, Zap, AlertTriangle, Trash2, Edit2, ExternalLink } from "lucide-preact";
import { listMonitors, listSamplesForMonitor, deleteMonitor, saveMonitor } from "../lib/db";
import { CLOUDFLARE_PRESET } from "../lib/probes/http";

export function Dashboard() {
    const [monitors, setMonitors] = useState<MonitorViewModel[]>([]);
    const [loading, setLoading] = useState(true);

    const loadData = async () => {
        const configs = await listMonitors();
        const models: MonitorViewModel[] = await Promise.all(
            configs.map(async (config) => {
                const samples = await listSamplesForMonitor(config.id, config.windowSize);
                const summary = computeSummary(config, samples);
                return { config, samples, summary };
            })
        );
        setMonitors(models);
        setLoading(false);
    };

    useEffect(() => {
        loadData();

        const bc = new BroadcastChannel("notmynet_updates");
        bc.onmessage = (msg) => {
            if (msg.data.type === "refresh") {
                loadData();
            }
            if (msg.data.type === "sample") {
                const sample = msg.data.sample as ProbeSample;
                setMonitors(prev => prev.map(m => {
                    if (m.config.id === sample.monitorId) {
                        const newSamples = [...m.samples, sample].slice(-m.config.windowSize);
                        return {
                            ...m,
                            samples: newSamples,
                            summary: computeSummary(m.config, newSamples, m.summary.totalWorstMs)
                        };
                    }
                    return m;
                }));
            }
        };

        return () => bc.close();
    }, []);

    if (loading) return <div class="p-8 text-center text-slate-400">Loading monitors...</div>;

    const handleLoadPreset = async () => {
        const now = Date.now();
        const config = {
            ...CLOUDFLARE_PRESET,
            id: crypto.randomUUID(),
            createdAt: now,
            updatedAt: now,
        } as any;
        await saveMonitor(config);

        // Notify worker
        const worker = (window as any).notmynet_worker;
        if (worker) {
            worker.port.postMessage({ type: "refresh" });
        }

        loadData();
    };

    if (monitors.length === 0) {
        return (
            <div class="p-12 text-center bg-slate-900 rounded-xl border border-slate-800 border-dashed">
                <Activity class="w-12 h-12 mx-auto mb-4 text-slate-600" />
                <h2 class="text-xl font-medium text-slate-200">No monitors yet</h2>
                <p class="text-slate-400 mt-2 mb-6">Create your first monitor to start tracking reachability.</p>
                <div class="flex flex-col sm:flex-row items-center justify-center gap-4">
                    <button
                        onClick={() => window.location.href = "/monitors/new"}
                        class="px-6 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg transition"
                    >
                        Add Monitor
                    </button>
                    <button
                        onClick={handleLoadPreset}
                        class="px-6 py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 rounded-lg transition"
                    >
                        Load Default Preset
                    </button>
                </div>
            </div>
        );
    }

    return (
        <div class="grid gap-6">
            {monitors.map(m => (
                <MonitorCard key={m.config.id} model={m} onDelete={loadData} />
            ))}
        </div>
    );
}

function MonitorCard({ model, onDelete }: { model: MonitorViewModel, onDelete: () => void }) {
    const { config, summary, samples } = model;

    const handleDelete = async (e: MouseEvent) => {
        e.stopPropagation();
        if (confirm(`Delete monitor "${config.name}"?`)) {
            await deleteMonitor(config.id);
            onDelete();
        }
    };

    const handleEdit = (e: MouseEvent) => {
        e.stopPropagation();
        window.location.href = `/monitors/edit?id=${config.id}`;
    };

    const handleDetail = () => {
        window.location.href = `/monitors/detail?id=${config.id}`;
    };

    return (
        <div
            onClick={handleDetail}
            class="bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-lg hover:border-slate-700 transition cursor-pointer group"
        >
            <div class="p-5">
                <div class="flex items-start justify-between mb-4">
                    <div>
                        <div class="flex items-center gap-2">
                            <h3 class="text-lg font-semibold text-slate-100 group-hover:text-blue-400 transition">{config.name}</h3>
                            <ExternalLink class="w-3.5 h-3.5 text-slate-600 group-hover:text-blue-400 transition opacity-0 group-hover:opacity-100" />
                        </div>
                        <p class="text-sm text-slate-400 font-mono truncate max-w-md">{config.url}</p>
                    </div>
                    <div class="flex items-center gap-3">
                        <StatusBadge state={summary.currentState} />
                        <div class="flex gap-1">
                            <button onClick={handleEdit} class="p-2 text-slate-400 hover:text-white hover:bg-slate-800 rounded-lg transition" title="Edit">
                                <Edit2 class="w-4 h-4" />
                            </button>
                            <button onClick={handleDelete} class="p-2 text-slate-400 hover:text-red-400 hover:bg-red-900/20 rounded-lg transition" title="Delete">
                                <Trash2 class="w-4 h-4" />
                            </button>
                        </div>
                    </div>
                </div>

                <div class="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-4 mb-6">
                    <Stat label="Streak" value={`${summary.streakCount} ${summary.streakType === "success" ? "UP" : "DOWN"}`} icon={<Zap class="w-3 h-3" />} />
                    <Stat label="Duration" value={formatDuration(summary.currentDurationMs)} icon={<Clock class="w-3 h-3" />} />
                    <Stat label="Avg Latency" value={summary.rollingAvgMs ? `${summary.rollingAvgMs}ms` : "N/A"} icon={<Activity class="w-3 h-3" />} />
                    <Stat label="Worst (Roll)" value={summary.rollingWorstMs ? `${summary.rollingWorstMs}ms` : "N/A"} icon={<AlertTriangle class="w-3 h-3" />} />
                    <Stat label="Worst (Total)" value={summary.totalWorstMs ? `${summary.totalWorstMs}ms` : "N/A"} icon={<AlertTriangle class="w-3 h-3" />} class="hidden md:block lg:block" />
                    <Stat label="Last Probe" value={summary.lastProbeAt ? new Date(summary.lastProbeAt).toLocaleTimeString([], {hour: '2-digit', minute:'2-digit', second:'2-digit'}) : "N/A"} icon={<Clock class="w-3 h-3" />} class="hidden md:block lg:block" />
                </div>

                <div class="flex gap-[2px] h-8 w-full bg-slate-950 rounded overflow-hidden p-[2px]">
                    {Array.from({ length: config.windowSize }).map((_, i) => {
                        const sample = samples[i + samples.length - config.windowSize];
                        if (!sample) return <div key={i} class="flex-1 bg-slate-800/30 rounded-sm"></div>;
                        return (
                            <div
                                key={sample.id}
                                class={`flex-1 rounded-sm ${getColorForState(sample.state, sample.elapsedMs, config.degradedMs)}`}
                                title={sample.elapsedMs ? `${sample.elapsedMs}ms` : sample.errorKind || "Outage"}
                            ></div>
                        );
                    })}
                </div>
            </div>
        </div>
    );
}

function StatusBadge({ state }: { state: ProbeState }) {
    const colors = {
        online: "bg-emerald-500/10 text-emerald-400 border-emerald-500/20",
        degraded: "bg-orange-500/10 text-orange-400 border-orange-500/20",
        outage: "bg-red-500/10 text-red-400 border-red-500/20",
    };

    return (
        <span class={`px-2.5 py-0.5 rounded-full text-xs font-bold border uppercase tracking-wider ${colors[state]}`}>
            {state}
        </span>
    );
}

function Stat({ label, value, icon, class: className = "" }: { label: string, value: string, icon: any, class?: string }) {
    return (
        <div class={`bg-slate-950/50 rounded-lg p-2 border border-slate-800/50 ${className}`}>
            <div class="flex items-center gap-1.5 text-[10px] uppercase font-bold text-slate-500 mb-1">
                {icon}
                {label}
            </div>
            <div class="text-xs font-semibold text-slate-200 truncate">{value}</div>
        </div>
    );
}
