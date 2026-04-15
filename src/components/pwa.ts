import { registerSW } from "virtual:pwa-register";

window.addEventListener("load", () => {
    const toast = document.querySelector<HTMLDivElement>("#pwa-toast");
    const message =
        document.querySelector<HTMLParagraphElement>("#toast-message");
    const refreshButton =
        document.querySelector<HTMLButtonElement>("#pwa-refresh");
    const closeButton = document.querySelector<HTMLButtonElement>("#pwa-close");

    if (!toast || !message || !refreshButton || !closeButton) return;

    let updateServiceWorker:
        | ((reloadPage?: boolean) => Promise<void>)
        | undefined;

    const show = (text: string, canRefresh: boolean) => {
        message.textContent = text;
        toast.classList.remove("hidden", "pointer-events-none");
        refreshButton.classList.toggle("hidden", !canRefresh);
    };

    const hide = () => {
        toast.classList.add("hidden", "pointer-events-none");
    };

    closeButton.addEventListener("click", hide);
    refreshButton.addEventListener("click", () => {
        void updateServiceWorker?.(true);
    });

    updateServiceWorker = registerSW({
        immediate: true,
        onOfflineReady() {
            show("App shell is ready to work offline.", false);
        },
        onNeedRefresh() {
            show("A new version is available.", true);
        },
        onRegisterError(error) {
            console.error("SW registration error", error);
        },
    });
});
