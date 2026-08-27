import { describe, it, expect } from 'vitest'
import { SiftFile } from '..'
import { EXIF_SAMPLES, EXIFTOOL_IMAGES, POPPLER_TEST, hasExifSamples, hasExifToolImages, hasPopplerTest, listFilesRecursive } from './helpers'
import { existsSync } from 'node:fs'
import { join } from 'node:path'

describe('thumbnail', () => {
  it.skipIf(!hasExifSamples())('DSCN0010.jpg returns JPEG thumbnail', () => {
    const path = join(EXIF_SAMPLES, 'jpg', 'gps', 'DSCN0010.jpg')
    if (!existsSync(path)) return

    const file = SiftFile.open(path)
    const doc = file.parse()
    const thumb = doc.thumbnail()
    expect(thumb).not.toBeNull()
    expect(thumb!.length).toBeGreaterThan(100)
    // JPEG SOI marker
    expect(thumb![0]).toBe(0xFF)
    expect(thumb![1]).toBe(0xD8)
    expect(thumb![2]).toBe(0xFF)
    doc.close()
  })

  it.skipIf(!hasExifSamples())('thumbnail ends with JPEG EOI', () => {
    const path = join(EXIF_SAMPLES, 'jpg', 'gps', 'DSCN0010.jpg')
    if (!existsSync(path)) return

    const file = SiftFile.open(path)
    const doc = file.parse()
    const thumb = doc.thumbnail()
    expect(thumb).not.toBeNull()
    // JPEG EOI marker
    expect(thumb![thumb!.length - 2]).toBe(0xFF)
    expect(thumb![thumb!.length - 1]).toBe(0xD9)
    doc.close()
  })

  it.skipIf(!hasExifSamples())('scans JPEGs - some have thumbnails', () => {
    const jpegDir = join(EXIF_SAMPLES, 'jpg')
    const files = listFilesRecursive(jpegDir, '.jpg')
    if (files.length === 0) return

    let withThumb = 0
    for (const path of files) {
      try {
        const file = SiftFile.open(path)
        const doc = file.parse()
        if (doc.thumbnail() !== null) withThumb++
        doc.close()
      } catch {}
    }
    expect(withThumb).toBeGreaterThan(0)
  })

  it.skipIf(!hasPopplerTest())('PDF returns null thumbnail', () => {
    const paths = listFilesRecursive(POPPLER_TEST, '.pdf')
    if (paths.length === 0) return

    const file = SiftFile.open(paths[0])
    const doc = file.parse()
    expect(doc.thumbnail()).toBeNull()
    doc.close()
  })

  it.skipIf(!hasExifToolImages())('no crash on file without thumbnail', () => {
    const file = SiftFile.open(join(EXIFTOOL_IMAGES, 'Canon.jpg'))
    const doc = file.parse()
    doc.thumbnail() // just verify no crash
    doc.close()
  })
})
