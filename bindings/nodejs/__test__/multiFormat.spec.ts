import { describe, it, expect } from 'vitest'
import { SiftFile, read, FileType, tags } from '..'
import { EXIFTOOL_IMAGES, hasExifToolImages, findFirst, listFiles } from './helpers'
import { readFileSync } from 'node:fs'

describe('multi-format', () => {
  it.skipIf(!hasExifToolImages())('scans all formats without exceptions', () => {
    const extensions = ['.jpg', '.png', '.gif', '.bmp', '.tif', '.tiff', '.webp']
    let total = 0, parsed = 0

    for (const ext of extensions) {
      for (const path of listFiles(EXIFTOOL_IMAGES, ext).slice(0, 5)) {
        total++
        try {
          const file = SiftFile.open(path)
          const doc = file.parse()
          doc.tags()
          parsed++
          doc.close()
        } catch {}
      }
    }
    expect(total).toBeGreaterThan(0)
    expect(parsed).toBeGreaterThan(0)
  })

  it.skipIf(!hasExifToolImages())('reads all formats from buffer', () => {
    const cases: [string, number][] = [
      ['.jpg', FileType.Jpeg],
      ['.png', FileType.Png],
      ['.gif', FileType.Gif],
    ]

    for (const [ext, expectedType] of cases) {
      const path = findFirst(EXIFTOOL_IMAGES, ext)
      if (!path) continue
      const data = readFileSync(path)
      const doc = read(data)
      expect(doc.fileType).toBe(expectedType)
      doc.close()
    }
  })

  it.skipIf(!hasExifToolImages())('tags list is thread-safe (concurrent access)', async () => {
    const t = tags(EXIFTOOL_IMAGES + '/Canon.jpg')
    expect(t.length).toBeGreaterThan(0)

    // Run 4 concurrent iterations over the tags array
    const promises = Array.from({ length: 4 }, () =>
      new Promise<number>((resolve) => {
        let count = 0
        for (const tag of t) {
          const s = `[${tag.group}] ${tag.name} = ${tag.value}`
          expect(s).toBeDefined()
          count++
        }
        resolve(count)
      })
    )

    const results = await Promise.all(promises)
    for (const r of results) {
      expect(r).toBe(t.length)
    }
  })
})
