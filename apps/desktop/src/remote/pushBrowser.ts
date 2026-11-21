/** A base64url VAPID key as the bytes `pushManager.subscribe` wants. */
export function keyBytes(base64url: string): Uint8Array {
  const padded = base64url.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(base64url.length / 4) * 4, '=')
  const binary = atob(padded)
  return Uint8Array.from(binary, (char) => char.charCodeAt(0))
}

const encoded = (key: ArrayBuffer | null) => key ? btoa(String.fromCharCode(...new Uint8Array(key))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '') : ''

export const pushSupported = () => typeof navigator !== 'undefined' && 'serviceWorker' in navigator && typeof Notification !== 'undefined' && typeof PushManager !== 'undefined'
export const pushPermission = () => Notification.permission
export const requestPushPermission = () => Notification.requestPermission()
export async function hasPushSubscription() {
  const registration = await navigator.serviceWorker.ready
  return Boolean(await registration.pushManager.getSubscription())
}
export async function subscribeToPush(publicKey: Uint8Array) {
  const registration = await navigator.serviceWorker.ready
  const subscription = await registration.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: publicKey as BufferSource })
  return { endpoint: subscription.endpoint, p256dh: encoded(subscription.getKey('p256dh')), auth: encoded(subscription.getKey('auth')) }
}
export async function unsubscribeFromPush() {
  const registration = await navigator.serviceWorker.ready
  await (await registration.pushManager.getSubscription())?.unsubscribe()
}
