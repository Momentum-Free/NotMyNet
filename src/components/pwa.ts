import { registerSW } from 'virtual:pwa-register'

const updateSW = registerSW({
  onNeedRefresh() {
    showToast('New content available, click on reload button to update.', true)
  },
  onOfflineReady() {
    showToast('App shell is ready to work offline.')
  },
})

function showToast(message: string, showRefresh = false) {
  const toast = document.getElementById('pwa-toast')
  const toastMessage = document.getElementById('toast-message')
  const refreshBtn = document.getElementById('pwa-refresh') as HTMLButtonElement

  if (!toast || !toastMessage) return

  toast.classList.remove('hidden')
  toast.classList.add('flex', 'flex-col', 'pointer-events-auto')
  toastMessage.textContent = message

  if (showRefresh && refreshBtn) {
    refreshBtn.classList.remove('hidden')
  }
}

document.addEventListener('DOMContentLoaded', () => {
    const closeBtn = document.getElementById('pwa-close')
    const refreshBtn = document.getElementById('pwa-refresh')

    closeBtn?.addEventListener('click', () => {
        document.getElementById('pwa-toast')?.classList.add('hidden')
    })

    refreshBtn?.addEventListener('click', () => {
        updateSW(true)
    })
})
