export function formatSeedInput(input: string): string {
  const letters = input.replace(/[^a-z]/gi, '').slice(0, 9).toUpperCase()
  return letters.match(/.{1,3}/g)?.join('-') ?? ''
}

export function compactNumber(value: number): string {
  if (value < 1_000) return Math.round(value).toLocaleString()
  const units = [['T', 1e12], ['B', 1e9], ['M', 1e6], ['K', 1e3]] as const
  const [suffix, divisor] = units.find(([, divisor]) => value >= divisor) ?? ['K', 1e3]
  return `${(value / divisor).toFixed(value / divisor >= 100 ? 0 : 2).replace(/\.00$/, '')} ${suffix}`
}

export function formatDuration(milliseconds: number): string {
  const seconds = Math.floor(milliseconds / 1_000)
  if (seconds < 60) return `${seconds}s`
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
}

export function probabilityLabel(probability: number | null): string {
  if (!probability || probability <= 0) return 'Probability unavailable'
  return `Match probability ≈ 1 in ${compactNumber(1 / probability)}`
}
