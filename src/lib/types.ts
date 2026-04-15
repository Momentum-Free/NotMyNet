export type MonitorConfig = {
    id: string;
    name: string;
    type: "http";
    url: string;
    method: "GET" | "HEAD";
    intervalMs: number;
    timeoutMs: number;
    windowSize: number;
    expectedStatus: number[];
    degradedMs: number;
    enabled: boolean;
    createdAt: number;
    updatedAt: number;
};

export type ProbeSample = {
    id: string;
    monitorId: string;
    startedAt: number;
    finishedAt: number;
    elapsedMs?: number;
    ok: boolean;
    state: "online" | "degraded" | "outage";
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
    currentState: "online" | "degraded" | "outage";
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
    monitor: MonitorConfig;
    summary: MonitorSummary;
    samples: ProbeSample[];
};

export type AppSettings = {
    defaultWindowSize: number;
    defaultIntervalMs: number;
    defaultTimeoutMs: number;
    weakOnlineHint: boolean;
};

export type ExportBundle = {
    version: 1;
    exportedAt: number;
    settings: AppSettings;
    monitors: MonitorConfig[];
    samples: ProbeSample[];
};

export const DEFAULT_SETTINGS: AppSettings = {
    defaultWindowSize: 60,
    defaultIntervalMs: 1_000,
    defaultTimeoutMs: 1_000,
    weakOnlineHint: true,
};
