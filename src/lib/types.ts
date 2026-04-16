export type MonitorConfig = {
    id: string;
    name: string;
    type: "http";
    url: string;
    method: "GET" | "HEAD";
    intervalMs: number;
    timeoutMs: number;
    windowSize: number;
    expectedStatuses: number[];
    degradedMs: number;
    enabled: boolean;
    createdAt: number;
    updatedAt: number;
};

export type ProbeState = "online" | "degraded" | "outage";

export type ProbeSample = {
    id: string;
    monitorId: string;
    startedAt: number;
    finishedAt: number;
    elapsedMs?: number;
    ok: boolean;
    state: ProbeState;
    httpStatus?: number;
    errorKind?:
        | "timeout"
        | "network"
        | "cors"
        | "http"
        | "opaque"
        | "offline_hint";
};

export type MonitorSummary = {
    monitorId: string;
    currentState: ProbeState;
    currentDurationMs: number;
    streakType: "success" | "failure";
    streakCount: number;
    rollingAvgMs?: number;
    rollingWorstMs?: number;
    totalWorstMs?: number;
    lastProbeAt?: number;
    lastSuccessAt?: number;
};

export type MonitorViewModel = {
    config: MonitorConfig;
    summary: MonitorSummary;
    samples: ProbeSample[];
};

export type AppSettings = {
    defaultWindowSize: number;
    defaultIntervalMs: number;
    defaultTimeoutMs: number;
    retentionDays: number;
    weakOnlineHint: boolean;
};

export type ExportBundle = {
    version: 1;
    exportedAt: number;
    settings: AppSettings;
    monitors: MonitorConfig[];
    samples?: ProbeSample[];
};

export const DEFAULT_SETTINGS: AppSettings = {
    defaultWindowSize: 60,
    defaultIntervalMs: 5000, // 5 seconds default seems reasonable
    defaultTimeoutMs: 5000,
    retentionDays: 30,
    weakOnlineHint: true,
};
