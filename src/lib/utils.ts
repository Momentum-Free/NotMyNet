import type { MonitorConfig, MonitorSummary, ProbeSample, ProbeState } from "./types";

export function computeSummary(
    config: MonitorConfig,
    samples: ProbeSample[],
    allTimeWorstMs?: number
): MonitorSummary {
    if (samples.length === 0) {
        return {
            monitorId: config.id,
            currentState: "online",
            currentDurationMs: 0,
            streakType: "success",
            streakCount: 0,
        };
    }

    const latest = samples[samples.length - 1];

    // Streak and duration
    let streakCount = 0;
    let stateSince = latest.startedAt;
    for (let i = samples.length - 1; i >= 0; i--) {
        if (samples[i].state !== latest.state) break;
        streakCount++;
        stateSince = samples[i].startedAt;
    }

    const successful = samples.filter((s) => s.state !== "outage");
    const measurable = samples.filter((s) => s.elapsedMs !== undefined);

    const rollingAvgMs = measurable.length > 0
        ? Math.round(measurable.reduce((sum, s) => sum + (s.elapsedMs ?? 0), 0) / measurable.length)
        : undefined;

    const rollingWorstMs = measurable.length > 0
        ? Math.max(...measurable.map((s) => s.elapsedMs ?? 0))
        : undefined;

    return {
        monitorId: config.id,
        currentState: latest.state,
        currentDurationMs: Date.now() - stateSince,
        streakType: latest.state === "outage" ? "failure" : "success",
        streakCount,
        rollingAvgMs,
        rollingWorstMs,
        totalWorstMs: allTimeWorstMs ? Math.max(allTimeWorstMs, rollingWorstMs ?? 0) : rollingWorstMs,
        lastProbeAt: latest.startedAt,
        lastSuccessAt: successful.length > 0 ? successful[successful.length - 1].startedAt : undefined,
    };
}

export function getColorForState(state: ProbeState, latency?: number, degradedMs: number = 500) {
    if (state === "outage") return "bg-red-500";
    if (state === "degraded") return "bg-orange-500";

    if (latency !== undefined) {
        if (latency < degradedMs * 0.2) return "bg-emerald-400";
        if (latency < degradedMs * 0.5) return "bg-emerald-500";
        if (latency < degradedMs * 0.8) return "bg-green-500";
        return "bg-yellow-500";
    }

    return "bg-emerald-500";
}

export function formatDuration(ms: number) {
    if (ms < 1000) return `${ms}ms`;
    const seconds = Math.floor(ms / 1000);
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
    const hours = Math.floor(minutes / 60);
    return `${hours}h ${minutes % 60}m`;
}
