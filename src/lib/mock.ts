import type {
    MonitorDefinition,
    MonitorSummary,
    MonitorViewModel,
    ProbeSample,
    ProbeState,
} from "./types";

const now = Date.now();
const WINDOW_SIZE = 60;

function sampleId() {
    return crypto.randomUUID();
}

function buildSamples(
    monitor: MonitorDefinition,
    baseLatency: number,
    overrides: Record<number, ProbeState>,
) {
    const samples: ProbeSample[] = [];

    for (let i = 0; i < WINDOW_SIZE; i += 1) {
        const state = overrides[i] ?? "up";
        const startedAt = now - (WINDOW_SIZE - i) * monitor.intervalMs;
        const endedAt = startedAt + 50;

        let latencyMs: number | null = baseLatency + ((i * 7) % 9);
        let httpStatus: number | null = 200;
        let ok = true;
        let error: string | null = null;

        if (state === "degraded") {
            latencyMs = baseLatency + 90 + (i % 25);
        }

        if (state === "down") {
            latencyMs = null;
            httpStatus = null;
            ok = false;
            error = "Network request failed";
        }

        samples.push({
            id: sampleId(),
            monitorId: monitor.id,
            startedAt,
            endedAt,
            latencyMs,
            httpStatus,
            ok,
            state,
            error,
        });
    }

    return samples;
}

function summarize(samples: ProbeSample[]): MonitorSummary {
    const latest = samples[samples.length - 1];
    let streakCount = 0;
    let stateSince = latest.startedAt;

    for (let i = samples.length - 1; i >= 0; i -= 1) {
        if (samples[i].state !== latest.state) break;
        streakCount += 1;
        stateSince = samples[i].startedAt;
    }

    const successful = samples.filter((sample) => sample.state !== "down");
    const measurable = samples.filter((sample) => sample.latencyMs !== null);
    const lastLatency = [...measurable].reverse()[0]?.latencyMs ?? null;

    const avg =
        measurable.length > 0
            ? Math.round(
                  measurable.reduce(
                      (sum, sample) => sum + (sample.latencyMs ?? 0),
                      0,
                  ) / measurable.length,
              )
            : null;

    const worst =
        measurable.length > 0
            ? Math.max(...measurable.map((sample) => sample.latencyMs ?? 0))
            : null;

    return {
        rollingAvailability: Number(
            ((successful.length / samples.length) * 100).toFixed(2),
        ),
        currentState: latest.state,
        stateSince,
        streakCount,
        currentDurationMs: now - stateSince,
        lastLatencyMs: lastLatency,
        rollingAverageLatencyMs: avg,
        rollingWorstLatencyMs: worst,
        allTimeWorstLatencyMs: worst ? worst + 17 : null,
    };
}

function makeMonitor(
    input: Pick<MonitorDefinition, "id" | "name" | "url"> & {
        baseLatency: number;
    },
    overrides: Record<number, ProbeState>,
): MonitorViewModel {
    const monitor: MonitorDefinition = {
        id: input.id,
        name: input.name,
        url: input.url,
        method: "GET",
        intervalMs: 10_000,
        timeoutMs: 4_000,
        expectedStatuses: [200, 204],
        latencyWarnMs: 120,
        enabled: true,
        createdAt: now - 86400000,
        updatedAt: now,
    };

    const samples = buildSamples(monitor, input.baseLatency, overrides);

    return {
        monitor,
        samples,
        summary: summarize(samples),
    };
}

export const mockMonitors: MonitorViewModel[] = [
    makeMonitor(
        {
            id: "cf-dns",
            name: "Cloudflare DNS",
            url: "https://1.1.1.1/",
            baseLatency: 13,
        },
        {
            18: "degraded",
            43: "degraded",
        },
    ),
    makeMonitor(
        {
            id: "router",
            name: "Gateway / Router UI",
            url: "http://192.168.1.1/",
            baseLatency: 4,
        },
        {},
    ),
    makeMonitor(
        {
            id: "game-api",
            name: "Game API / Session Edge",
            url: "https://example-game-edge.test/health",
            baseLatency: 31,
        },
        {
            7: "down",
            8: "down",
            9: "down",
            22: "degraded",
            48: "degraded",
        },
    ),
];
