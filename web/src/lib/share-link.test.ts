import { describe, expect, it } from 'vitest'
import { hasShareCode, withoutFragment } from './share-link'

describe('share link fragments', () => {
  it('detects share codes in fragments', () => {
    expect(hasShareCode('#q=EAGWhMA')).toBe(true)
    expect(hasShareCode('#other&q=EAGWhMA')).toBe(true)
    expect(hasShareCode('')).toBe(false)
    expect(hasShareCode('#')).toBe(false)
    expect(hasShareCode('#squire')).toBe(false)
    expect(hasShareCode('#faq=1')).toBe(false)
  })

  it('strips the fragment from an href', () => {
    expect(withoutFragment('https://x.app/#q=EAGWhMA')).toBe('https://x.app/')
    expect(withoutFragment('https://x.app/')).toBe('https://x.app/')
  })
})
