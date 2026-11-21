// These RPCs are simulated, but their native browser/host boundary cannot be demonstrated without a paired device.
export const undemoedRpcs: Readonly<Record<string, string>> = {
  'push.subscribe': 'Real registration requires a paired device, browser permission and a push service subscription.',
  'push.unsubscribe': 'Real removal requires a subscription registered with a push service and the paired host.',
}
