export type RegionName = 'Sewers' | 'Prison' | 'Caves' | 'Dwarven City' | 'Demon Halls'
export interface Region { name: RegionName; color: string }

export function regionForDepth(depth: number): Region {
  if (depth <= 5) return { name: 'Sewers', color: '#7FE2B8' }
  if (depth <= 10) return { name: 'Prison', color: '#8FB7E8' }
  if (depth <= 15) return { name: 'Caves', color: '#D8A26B' }
  if (depth <= 20) return { name: 'Dwarven City', color: '#C9A6E8' }
  return { name: 'Demon Halls', color: '#E88F8F' }
}

export const floorLabel = (depth: number): string => `Floor ${depth} · ${regionForDepth(depth).name}`
