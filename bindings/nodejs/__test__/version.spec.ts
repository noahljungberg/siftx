import { describe, it, expect } from 'vitest'
import { version } from '..'

describe('version', () => {
  it('returns valid string', () => {
    const v = version()
    expect(v).toBeDefined()
    expect(v).toBe('0.1.0')
  })
})
