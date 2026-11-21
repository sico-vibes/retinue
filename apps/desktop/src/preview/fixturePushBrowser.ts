export { keyBytes } from '../remote/pushBrowser'

const state = new URLSearchParams(location.search).get('demo-push')
let subscribed = state === 'on'

export const pushSupported = () => state !== null && state !== 'unsupported'
export const pushPermission = (): NotificationPermission => state === 'blocked' ? 'denied' : 'granted'
export const requestPushPermission = async () => pushPermission()
export const hasPushSubscription = async () => subscribed
export async function subscribeToPush(_publicKey: Uint8Array) {
  subscribed = true
  return { endpoint: 'https://fcm.googleapis.com/fcm/send/retinue-preview', p256dh: 'cHJldmlldw', auth: 'cHJldmlldw' }
}
export async function unsubscribeFromPush() { subscribed = false }
