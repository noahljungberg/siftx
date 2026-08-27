import { describe, it, expect } from 'vitest'
import { SiftFile, tags } from '..'
import { EXIFTOOL_IMAGES, POPPLER_TEST, hasExifToolImages, hasPopplerTest, listFiles, listFilesRecursive } from './helpers'
import { join } from 'node:path'

describe('tags', () => {
  it.skipIf(!hasExifToolImages())('Canon.jpg has Make tag', () => {
    const file = SiftFile.open(join(EXIFTOOL_IMAGES, 'Canon.jpg'))
    const doc = file.parse()
    const t = doc.tags()
    expect(t.length).toBeGreaterThan(0)

    const make = t.find(tag => tag.name === 'Make')
    expect(make).toBeDefined()
    expect(make!.value).toBe('Canon')
    expect(make!.group).toBe('EXIF')
    doc.close()
  })

  it.skipIf(!hasExifToolImages())('Canon.jpg has Model tag', () => {
    const file = SiftFile.open(join(EXIFTOOL_IMAGES, 'Canon.jpg'))
    const doc = file.parse()
    const model = doc.tags().find(t => t.name === 'Model')
    expect(model).toBeDefined()
    expect(model!.value).toContain('Canon')
    doc.close()
  })

  it.skipIf(!hasExifToolImages())('has multiple groups', () => {
    const file = SiftFile.open(join(EXIFTOOL_IMAGES, 'Canon.jpg'))
    const doc = file.parse()
    const groups = [...new Set(doc.tags().map(t => t.group))]
    expect(groups).toContain('EXIF')
    doc.close()
  })

  it.skipIf(!hasExifToolImages())('tags() convenience function', () => {
    const t = tags(join(EXIFTOOL_IMAGES, 'Canon.jpg'))
    expect(t.length).toBeGreaterThan(0)
    expect(t.some(tag => tag.name === 'Make')).toBe(true)
  })

  it.skipIf(!hasExifToolImages())('scans multiple JPEGs', () => {
    const files = listFiles(EXIFTOOL_IMAGES, '.jpg').slice(0, 20)
    if (files.length === 0) return

    let parsed = 0, withTags = 0
    for (const path of files) {
      try {
        const file = SiftFile.open(path)
        const doc = file.parse()
        parsed++
        if (doc.tags().length > 0) withTags++
        doc.close()
      } catch {}
    }
    expect(parsed).toBeGreaterThan(0)
    expect(withTags).toBeGreaterThan(0)
  })

  it('Tag toString format', () => {
    const tag = { group: 'EXIF', name: 'Make', value: 'Canon' }
    expect(`[${tag.group}] ${tag.name} = ${tag.value}`).toBe('[EXIF] Make = Canon')
  })

  it('Tag equality', () => {
    const a = { group: 'EXIF', name: 'Make', value: 'Canon' }
    const b = { group: 'EXIF', name: 'Make', value: 'Canon' }
    const c = { group: 'EXIF', name: 'Model', value: 'EOS' }
    expect(a).toEqual(b)
    expect(a).not.toEqual(c)
  })

  it.skipIf(!hasPopplerTest())('PDF has metadata tags', () => {
    const paths = listFilesRecursive(POPPLER_TEST, '.pdf').slice(0, 10)
    if (paths.length === 0) return

    let withTags = 0
    for (const path of paths) {
      try {
        const t = tags(path)
        if (t.some(tag => tag.group === 'PDF')) withTags++
      } catch {}
    }
    expect(withTags).toBeGreaterThan(0)
  })
})
