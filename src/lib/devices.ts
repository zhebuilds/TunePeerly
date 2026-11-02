// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

/**
 * Devices heard on the network that are not paired yet. A paired device is matched by its device id,
 * never by address, and stays out of Nearby whether it is online, offline or manually disconnected.
 */
export function unpairedNearby<T extends { deviceId: string }>(nearby: readonly T[], links: readonly { deviceId: string }[]): T[] {
  const paired = new Set(links.map((link) => link.deviceId));
  return nearby.filter((item) => !paired.has(item.deviceId));
}
