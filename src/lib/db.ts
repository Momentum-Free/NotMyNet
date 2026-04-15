import type { AppSettings, MonitorDefinition, ProbeSample } from "./types";
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
        tx.objectStore("monitors").getAll() as IDBRequest<MonitorDefinition[]>,
    );
}

export async function saveMonitor(monitor: MonitorDefinition) {
    const db = await openDb();
    const tx = db.transaction("monitors", "readwrite");
    await requestToPromise(tx.objectStore("monitors").put(monitor));
}

export async function addSample(sample: ProbeSample) {
    const db = await openDb();
    const tx = db.transaction("samples", "readwrite");
    await requestToPromise(tx.objectStore("samples").put(sample));
}

export async function listSamplesForMonitor(monitorId: string) {
    const db = await openDb();
    const tx = db.transaction("samples", "readonly");
    const index = tx.objectStore("samples").index("by-monitor");
    const samples = await requestToPromise(
        index.getAll(monitorId) as IDBRequest<ProbeSample[]>,
    );
    return samples.sort((a, b) => a.startedAt - b.startedAt);
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
