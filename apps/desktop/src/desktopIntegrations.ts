import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow, UserAttentionType } from '@tauri-apps/api/window'

/** How this copy was installed. A Microsoft Store copy starts at sign-in through its startup task. */
let storeCopy: Promise<boolean> | null = null
function isStoreCopy() {
  storeCopy ??= invoke<string>('app_distribution').then((value) => value === 'store').catch(() => false)
  return storeCopy
}

export async function autostartEnabled() {
  if (await isStoreCopy()) return invoke<boolean>('startup_task_enabled')
  return (await import('@tauri-apps/plugin-autostart')).isEnabled()
}
export async function setAutostartEnabled(enabled: boolean) {
  if (await isStoreCopy()) {
    await invoke<boolean>('set_startup_task_enabled', { enabled })
    return
  }
  const plugin = await import('@tauri-apps/plugin-autostart')
  return enabled ? plugin.enable() : plugin.disable()
}

export async function sendDesktopNotification(title: string, body: string) {
  const plugin = await import('@tauri-apps/plugin-notification')
  let granted = await plugin.isPermissionGranted()
  if (!granted) granted = (await plugin.requestPermission()) === 'granted'
  if (granted) plugin.sendNotification({ title, body })
}

export function flashMainWindow() {
  return getCurrentWindow().requestUserAttention(UserAttentionType.Critical)
}
