import ky from "ky";
import type { MonitorConfig, ProbeSample, ProbeState } from "../types";

function classifyState(
    monitor: MonitorConfig,
    httpStatus: number | undefined,
    elapsedMs: number | undefined,
): ProbeState {
    if (httpStatus === undefined || elapsedMs === undefined) return "outage";
    if (!monitor.expectedStatuses.includes(httpStatus)) return "outage";
    if (elapsedMs >= monitor.degradedMs) return "degraded";
    return "online";
}

export async function runHttpProbe(
    monitor: MonitorConfig,
): Promise<ProbeSample> {
    const startedAt = Date.now();
    const t0 = performance.now();

    try {
        const response = await ky(monitor.url, {
            method: monitor.method,
            timeout: monitor.timeoutMs,
            retry: 0,
            cache: "no-store",
            throwHttpErrors: false,
            headers: {
                "cache-control": "no-cache",
            },
        });

        const finishedAt = Date.now();
        const elapsedMs = Math.round(performance.now() - t0);

        // Check for opaque response
        if (response.type === "opaque") {
            return {
                id: crypto.randomUUID(),
                monitorId: monitor.id,
                startedAt,
                finishedAt,
                ok: false,
                state: "outage",
                errorKind: "opaque",
            };
        }

        const state = classifyState(monitor, response.status, elapsedMs);
        const ok = state !== "outage";

        return {
            id: crypto.randomUUID(),
            monitorId: monitor.id,
            startedAt,
            finishedAt,
            elapsedMs,
            httpStatus: response.status,
            ok,
            state,
            errorKind: ok ? undefined : "http",
        };
    } catch (error: any) {
        const finishedAt = Date.now();

        let errorKind: ProbeSample["errorKind"] = "network";
        if (error.name === "TimeoutError") {
            errorKind = "timeout";
        } else if (!navigator.onLine) {
            errorKind = "offline_hint";
        }
        // Identifying CORS specifically is hard, usually falls under "network"

        return {
            id: crypto.randomUUID(),
            monitorId: monitor.id,
            startedAt,
            finishedAt,
            ok: false,
            state: "outage",
            errorKind,
        };
    }
}

export const CLOUDFLARE_PRESET: Omit<MonitorConfig, "id" | "createdAt" | "updatedAt"> = {
    name: "Cloudflare (Internet Reachability)",
    type: "http",
    url: "https://1.1.1.1/cdn-cgi/trace",
    method: "GET",
    intervalMs: 10000,
    timeoutMs: 5000,
    windowSize: 60,
    expectedStatuses: [200],
    degradedMs: 500,
    enabled: true,
};
