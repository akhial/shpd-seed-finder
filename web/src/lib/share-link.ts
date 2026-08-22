/** Detecting and clearing share-link fragments (`#q=CODE`). Decoding the code
 * itself happens in the engine, which accepts the full URL text. */

/** Whether a URL fragment carries a share code. */
export function hasShareCode(hash: string): boolean {
  return /(^#|[?&])q=/.test(hash)
}

/** The href with its fragment removed, for cleaning the address bar. */
export function withoutFragment(href: string): string {
  const cut = href.indexOf('#')
  return cut === -1 ? href : href.slice(0, cut)
}
