import { defineConfig } from "astro/config";
import tailwindcss from "@tailwindcss/vite";
import AstroPWA from "@vite-pwa/astro";
import preact from "@astrojs/preact";

export default defineConfig({
    output: "static",
    vite: {
        plugins: [tailwindcss()],
    },
    integrations: [
        preact(),
        AstroPWA({
            registerType: "prompt",
            devOptions: {
                enabled: true,
            },
            workbox: {
                globPatterns: ["**/*.{js,css,html,svg,png,ico,webp}"],
            },
            manifest: {
                name: "NotMyNet",
                short_name: "NotMyNet",
                description: "Local-first browser-based reachability monitor",
                start_url: "/",
                display: "standalone",
                background_color: "#020617",
                theme_color: "#020617",
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
