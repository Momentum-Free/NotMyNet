import type { AppSettings, MonitorConfig, ProbeSample } from "./types";
import { DEFAULT_SETTINGS } from "./types";

const DB_NAME = "notmynet";
const DB_VERSION = 1;

type SettingsRow = AppSettings & { id: "app" };

function requestToPromise<T>(request: IDBRequest<T>) {
    return new Promise<T>((resolve, reject) => {
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
    });
}

export function openDb() {
    return new Promise<IDBDatabase>((resolve, reject) => {
        const request = indexedDB.open(DB_NAME, DB_VERSION);

        request.onupgradeneeded = () => {
            const db = request.result;

            if (!db.objectStoreNames.contains("monitors")) {
                db.createObjectStore("monitors", { keyPath: "id" });
            }

            if (!db.objectStoreNames.contains("samples")) {
                const samples = db.createObjectStore("samples", {
                    keyPath: "id",
                });
                samples.createIndex("by-monitor", "monitorId", {
                    unique: false,
                });
                samples.createIndex("by-startedAt", "startedAt", {
                    unique: false,
                });
                // Composite index for monitor samples ordered by time
                samples.createIndex(
                    "by-monitor-startedAt",
                    ["monitorId", "startedAt"],
                    {
                        unique: false,
                    },
                );
            }

            if (!db.objectStoreNames.contains("settings")) {
                db.createObjectStore("settings", { keyPath: "id" });
            }
        };

        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
    });
}

export async function listMonitors() {
    const db = await openDb();
    const tx = db.transaction("monitors", "readonly");
    return requestToPromise(
        tx.objectStore("monitors").getAll() as IDBRequest<MonitorConfig[]>,
    );
}

export async function saveMonitor(monitor: MonitorConfig) {
    const db = await openDb();
    const tx = db.transaction("monitors", "readwrite");
    await requestToPromise(tx.objectStore("monitors").put(monitor));
}

export async function deleteMonitor(id: string) {
    const db = await openDb();
    const tx = db.transaction(["monitors", "samples"], "readwrite");
    await requestToPromise(tx.objectStore("monitors").delete(id));

    // Also delete samples for this monitor
    const samplesStore = tx.objectStore("samples");
    const index = samplesStore.index("by-monitor");
    const request = index.openKeyCursor(IDBKeyRange.only(id));
    request.onsuccess = () => {
        const cursor = request.result;
        if (cursor) {
            samplesStore.delete(cursor.primaryKey);
            cursor.continue();
        }
    };
}

export async function addSample(sample: ProbeSample) {
    const db = await openDb();
    const tx = db.transaction("samples", "readwrite");
    await requestToPromise(tx.objectStore("samples").put(sample));
}

export async function listSamplesForMonitor(monitorId: string, limit?: number) {
    const db = await openDb();
    const tx = db.transaction("samples", "readonly");
    const index = tx.objectStore("samples").index("by-monitor-startedAt");

    // We want the most recent ones if there is a limit
    const range = IDBKeyRange.bound([monitorId, 0], [monitorId, Date.now()]);
    const request = index.getAll(range, limit) as IDBRequest<ProbeSample[]>;
    const samples = await requestToPromise(request);

    // If we used limit, it might not be the most recent if we didn't use prev direction
    // Actually IDB getAll doesn't support direction. We should use cursor for that.
    if (limit) {
        const results: ProbeSample[] = [];
        return new Promise<ProbeSample[]>((resolve, reject) => {
            const cursorReq = index.openCursor(range, "prev");
            cursorReq.onsuccess = () => {
                const cursor = cursorReq.result;
                if (cursor && results.length < limit) {
                    results.push(cursor.value);
                    cursor.continue();
                } else {
                    resolve(results.reverse());
                }
            };
            cursorReq.onerror = () => reject(cursorReq.error);
        });
    }

    return samples.sort((a, b) => a.startedAt - b.startedAt);
}

export async function pruneOldSamples(retentionDays: number) {
    const db = await openDb();
    const tx = db.transaction("samples", "readwrite");
    const store = tx.objectStore("samples");
    const index = store.index("by-startedAt");
    const cutoff = Date.now() - retentionDays * 24 * 60 * 60 * 1000;
    const range = IDBKeyRange.upperBound(cutoff);

    const request = index.openKeyCursor(range);
    request.onsuccess = () => {
        const cursor = request.result;
        if (cursor) {
            store.delete(cursor.primaryKey);
            cursor.continue();
        }
    };
}

export async function getSettings() {
    const db = await openDb();
    const tx = db.transaction("settings", "readonly");
    const row = await requestToPromise(
        tx.objectStore("settings").get("app") as IDBRequest<
            SettingsRow | undefined
        >,
    );
    if (!row) return DEFAULT_SETTINGS;
    const { id: _id, ...settings } = row;
    return settings;
}

export async function saveSettings(settings: AppSettings) {
    const db = await openDb();
    const tx = db.transaction("settings", "readwrite");
    const row: SettingsRow = { id: "app", ...settings };
    await requestToPromise(tx.objectStore("settings").put(row));
}
