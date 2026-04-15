import { h } from "preact";
import { useState, useEffect } from "preact/hooks";
import type { MonitorViewModel, ProbeSample } from "../lib/types";
import { computeSummary, getColorForState, formatDuration } from "../lib/utils";
import { Activity, Clock, Zap, AlertTriangle, ChevronLeft, Calendar, BarChart3, List } from "lucide-preact";
import { listMonitors, listSamplesForMonitor } from "../lib/db";

export function MonitorDetail({ id }: { id: string }) {
    const [model, setModel] = useState<MonitorViewModel | null>(null);
    const [loading, setLoading] = useState(true);

    const loadData = async () => {
        const configs = await listMonitors();
        const config = configs.find(m => m.id === id);
        if (!config) {
            setLoading(false);
            return;
        }

        const samples = await listSamplesForMonitor(config.id, 200); // More samples for detail view
        const summary = computeSummary(config, samples);
        setModel({ config, samples, summary });
        setLoading(false);
    };

    useEffect(() => {
        loadData();

        const bc = new BroadcastChannel("notmynet_updates");
        bc.onmessage = (msg) => {
            if (msg.data.type === "sample" && msg.data.sample.monitorId === id) {
                const sample = msg.data.sample as ProbeSample;
                setModel(prev => {
                    if (!prev) return null;
                    const newSamples = [...prev.samples, sample].slice(-200);
                    return {
                        ...prev,
                        samples: newSamples,
                        summary: computeSummary(prev.config, newSamples, prev.summary.totalWorstMs)
                    };
                });
            }
        };

        return () => bc.close();
    }, [id]);

    if (loading) return <div class="p-8 text-center text-slate-400">Loading details...</div>;
    if (!model) return <div class="p-8 text-center text-red-400">Monitor not found.</div>;

    const { config, summary, samples } = model;

    return (
        <div class="space-y-8">
            <header class="flex items-center justify-between">
                <div>
                    <h2 class="text-3xl font-bold text-slate-100">{config.name}</h2>
                    <p class="text-slate-400 font-mono text-sm mt-1">{config.url}</p>
                </div>
                <StatusBadge state={summary.currentState} />
            </header>

            <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                <DetailStat label="Current Streak" value={`${summary.streakCount} ${summary.streakType === "success" ? "UP" : "DOWN"}`} icon={<Zap class="w-4 h-4" />} />
                <DetailStat label="State Duration" value={formatDuration(summary.currentDurationMs)} icon={<Clock class="w-4 h-4" />} />
                <DetailStat label="Last Probe" value={summary.lastProbeAt ? new Date(summary.lastProbeAt).toLocaleTimeString() : "N/A"} icon={<Calendar class="w-4 h-4" />} />
                <DetailStat label="Last Success" value={summary.lastSuccessAt ? new Date(summary.lastSuccessAt).toLocaleTimeString() : "N/A"} icon={<Calendar class="w-4 h-4" />} />
            </div>

            <section class="bg-slate-900 border border-slate-800 rounded-xl p-6">
                <h3 class="text-lg font-semibold text-slate-100 mb-4 flex items-center gap-2">
                    <BarChart3 class="w-5 h-5 text-blue-400" />
                    Expanded Timeline (Last {samples.length} checks)
                </h3>
                <div class="flex flex-wrap gap-1 h-24 items-end bg-slate-950 p-3 rounded-lg border border-slate-800">
                    {samples.map(sample => (
                        <div
                            key={sample.id}
                            className={`flex-1 min-w-[4px] rounded-t-sm ${getColorForState(sample.state, sample.elapsedMs, config.degradedMs)}`}
                            style={{ height: sample.state === "outage" ? '100%' : sample.elapsedMs ? `${Math.min(100, (sample.elapsedMs / config.degradedMs) * 100)}%` : '20%' }}
                            title={`${sample.elapsedMs ? sample.elapsedMs + 'ms' : 'Outage'} @ ${new Date(sample.startedAt).toLocaleTimeString()}`}
                        ></div>
                    ))}
                </div>
                <div class="flex justify-between mt-2 text-[10px] uppercase font-bold text-slate-500 px-1">
                    <span>{samples.length > 0 ? new Date(samples[0].startedAt).toLocaleTimeString() : ""}</span>
                    <span>Now</span>
                </div>
            </section>

            <div class="grid grid-cols-1 lg:grid-cols-2 gap-8">
                <section class="bg-slate-900 border border-slate-800 rounded-xl p-6">
                    <h3 class="text-lg font-semibold text-slate-100 mb-4 flex items-center gap-2">
                        <Activity class="w-5 h-5 text-emerald-400" />
                        Latency Stats
                    </h3>
                    <div class="space-y-4">
                        <StatRow label="Rolling Average" value={summary.rollingAvgMs ? `${summary.rollingAvgMs}ms` : "N/A"} />
                        <StatRow label="Rolling Worst" value={summary.rollingWorstMs ? `${summary.rollingWorstMs}ms` : "N/A"} />
                        <StatRow label="All-time Worst" value={summary.totalWorstMs ? `${summary.totalWorstMs}ms` : "N/A"} />
                        <StatRow label="Degraded Threshold" value={`${config.degradedMs}ms`} />
                    </div>
                </section>

                <section class="bg-slate-900 border border-slate-800 rounded-xl p-6">
                    <h3 class="text-lg font-semibold text-slate-100 mb-4 flex items-center gap-2">
                        <List class="w-5 h-5 text-orange-400" />
                        Recent Events
                    </h3>
                    <div class="space-y-3 max-h-60 overflow-y-auto pr-2 custom-scrollbar">
                        {[...samples].reverse().slice(0, 10).map(sample => (
                            <div key={sample.id} class="flex items-center justify-between text-sm py-2 border-b border-slate-800 last:border-0">
                                <div class="flex items-center gap-3">
                                    <div class={`w-2 h-2 rounded-full ${getColorForState(sample.state, sample.elapsedMs, config.degradedMs)}`}></div>
                                    <span class="text-slate-300 font-medium">{sample.state.toUpperCase()}</span>
                                </div>
                                <div class="flex items-center gap-4 text-slate-500 font-mono text-xs">
                                    <span>{sample.elapsedMs ? `${sample.elapsedMs}ms` : (sample.errorKind || "Outage")}</span>
                                    <span>{new Date(sample.startedAt).toLocaleTimeString()}</span>
                                </div>
                            </div>
                        ))}
                    </div>
                </section>
            </div>
        </div>
    );
}

function StatusBadge({ state }: { state: any }) {
    const colors = {
        online: "bg-emerald-500/10 text-emerald-400 border-emerald-500/20",
        degraded: "bg-orange-500/10 text-orange-400 border-orange-500/20",
        outage: "bg-red-500/10 text-red-400 border-red-500/20",
    };

    return (
        <span class={`px-4 py-1 rounded-full text-sm font-bold border uppercase tracking-widest ${colors[state as keyof typeof colors]}`}>
            {state}
        </span>
    );
}

function DetailStat({ label, value, icon }: { label: string, value: string, icon: any }) {
    return (
        <div class="bg-slate-900 border border-slate-800 rounded-xl p-4">
            <div class="flex items-center gap-2 text-xs font-bold text-slate-500 uppercase mb-2">
                {icon}
                {label}
            </div>
            <div class="text-xl font-bold text-slate-100">{value}</div>
        </div>
    );
}

function StatRow({ label, value }: { label: string, value: string }) {
    return (
        <div class="flex items-center justify-between py-1">
            <span class="text-slate-400 text-sm">{label}</span>
            <span class="text-slate-200 font-mono font-bold">{value}</span>
        </div>
    );
}
