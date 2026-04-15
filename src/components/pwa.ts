import { registerSW } from 'virtual:pwa-register'

const toast = document.getElementById('pwa-toast')
const toastMessage = document.getElementById('toast-message')
const closeBtn = document.getElementById('pwa-close')
const refreshBtn = document.getElementById('pwa-refresh')

function showToast(message: string, showRefresh = false) {
  if (!toast || !toastMessage) return
  toast.classList.remove('hidden')
  toast.classList.add('flex', 'flex-col')
  toastMessage.textContent = message
  if (showRefresh && refreshBtn) {
    refreshBtn.classList.remove('hidden')
  }
}

const updateSW = registerSW({
  onNeedRefresh() {
    showToast('New content available, click on reload button to update.', true)
  },
  onOfflineReady() {
    showToast('App shell is ready to work offline.')
  },
})

closeBtn?.addEventListener('click', () => {
  toast?.classList.add('hidden')
})

refreshBtn?.addEventListener('click', () => {
  updateSW()
})
