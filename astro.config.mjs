import { defineConfig } from "astro/config";
import AstroPWA from "@vite-pwa/astro";

export default defineConfig({
    integrations: [
        AstroPWA({
            registerType: "autoUpdate",
            devOptions: {
                enabled: true,
            },
            manifest: {
                name: "NotMyNet",
                short_name: "NotMyNet",
                description: "Local-first connection monitor",
                start_url: "/",
                display: "standalone",
                background_color: "#0b1020",
                theme_color: "#0b1020",
                icons: [
                    {
                        src: "/pwa-192x192.png",
                        sizes: "192x192",
                        type: "image/png",
                    },
                    {
                        src: "/pwa-512x512.png",
                        sizes: "512x512",
                        type: "image/png",
                    },
                ],
            },
        }),
    ],
});
