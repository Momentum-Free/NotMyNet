import ky from "ky";
import type { MonitorDefinition, ProbeSample, ProbeState } from "../types";

function classifyState(
    monitor: MonitorDefinition,
    httpStatus: number | null,
    latencyMs: number | null,
): ProbeState {
    if (httpStatus === null || latencyMs === null) return "down";
    if (!monitor.expectedStatuses.includes(httpStatus)) return "down";
    if (latencyMs >= monitor.latencyWarnMs) return "degraded";
    return "up";
}

export async function runHttpProbe(
    monitor: MonitorDefinition,
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

        const endedAt = Date.now();
        const latencyMs = Math.round(performance.now() - t0);
        const state = classifyState(monitor, response.status, latencyMs);

        return {
            id: crypto.randomUUID(),
            monitorId: monitor.id,
            startedAt,
            endedAt,
            latencyMs,
            httpStatus: response.status,
            ok: state !== "down",
            state,
            error: null,
        };
    } catch (error) {
        const endedAt = Date.now();

        return {
            id: crypto.randomUUID(),
            monitorId: monitor.id,
            startedAt,
            endedAt,
            latencyMs: null,
            httpStatus: null,
            ok: false,
            state: "down",
            error: error instanceof Error ? error.message : "Probe failed",
        };
    }
}
