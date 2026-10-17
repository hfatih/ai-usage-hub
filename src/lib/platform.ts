/** True when the interface runs on macOS (desktop app or a browser preview on a Mac). */
export const isMac = typeof navigator !== "undefined"
  && /mac/i.test((navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform ?? navigator.platform ?? navigator.userAgent);

/** Picks the wording that matches the user's operating system. */
export function byPlatform<T>(windows: T, mac: T): T {
  return isMac ? mac : windows;
}
