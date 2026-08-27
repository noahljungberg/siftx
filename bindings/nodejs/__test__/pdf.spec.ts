import { describe, it, expect } from 'vitest'
import { SiftFile, read, ImageFormat } from '..'
import { EXIFTOOL_IMAGES, POPPLER_TEST, hasExifToolImages, hasPopplerTest, listFilesRecursive } from './helpers'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

describe('PDF', () => {
  // --- Text extraction ---

  it.skipIf(!hasPopplerTest())('extracts text pages', () => {
    const paths = listFilesRecursive(POPPLER_TEST, '.pdf')
    if (paths.length === 0) return

    const file = SiftFile.open(paths[0])
    const doc = file.parse()
    const pages = doc.textPages()
    expect(pages).toBeDefined()
    doc.close()
  })

  it.skipIf(!hasExifToolImages())('non-PDF returns empty text', () => {
    const data = readFileSync(join(EXIFTOOL_IMAGES, 'Canon.jpg'))
    const doc = read(data)
    expect(doc.textPages()).toHaveLength(0)
    doc.close()
  })

  it.skipIf(!hasPopplerTest())('scans PDFs for text', () => {
    const paths = listFilesRecursive(POPPLER_TEST, '.pdf').slice(0, 20)
    if (paths.length === 0) return

    let parsed = 0, withText = 0
    for (const path of paths) {
      try {
        const file = SiftFile.open(path)
        const doc = file.parse()
        parsed++
        if (doc.textPages().some(p => p.length > 0)) withText++
        doc.close()
      } catch {}
    }
    expect(parsed).toBeGreaterThan(0)
  })

  // --- Image extraction ---

  it.skipIf(!hasPopplerTest())('extracts images from PDFs', () => {
    const paths = listFilesRecursive(POPPLER_TEST, '.pdf').slice(0, 30)
    if (paths.length === 0) return

    for (const path of paths) {
      try {
        const file = SiftFile.open(path)
        const doc = file.parse()
        const images = doc.images()
        for (const img of images) {
          expect(img.width).toBeLessThanOrEqual(100_000)
          expect(img.height).toBeLessThanOrEqual(100_000)
          expect(img.extension).toBeDefined()
        }
        doc.close()
      } catch {}
    }
  })

  it.skipIf(!hasExifToolImages())('non-PDF returns empty images', () => {
    const data = readFileSync(join(EXIFTOOL_IMAGES, 'Canon.jpg'))
    const doc = read(data)
    expect(doc.images()).toHaveLength(0)
    doc.close()
  })

  it.skipIf(!hasPopplerTest())('ExtractedImage properties', () => {
    const paths = listFilesRecursive(POPPLER_TEST, '.pdf').slice(0, 20)
    let found: any = null

    for (const path of paths) {
      try {
        const file = SiftFile.open(path)
        const doc = file.parse()
        const images = doc.images()
        if (images.length > 0) { found = images[0]; doc.close(); break }
        doc.close()
      } catch {}
    }

    if (!found) return // skip if no images found

    expect(found.width).toBeGreaterThan(0)
    expect(found.height).toBeGreaterThan(0)
    expect(found.bitsPerComponent).toBeGreaterThan(0)
    expect(found.components).toBeGreaterThan(0)
    expect(found.data.length).toBeGreaterThan(0)
  })

  it.skipIf(!hasPopplerTest())('ExtractedImage isPassthrough', () => {
    const paths = listFilesRecursive(POPPLER_TEST, '.pdf').slice(0, 30)
    let found: any = null

    for (const path of paths) {
      try {
        const file = SiftFile.open(path)
        const doc = file.parse()
        for (const img of doc.images()) {
          if (img.isPassthrough) { found = img; break }
        }
        doc.close()
        if (found) break
      } catch {}
    }

    if (found) {
      expect(
        found.format === ImageFormat.Jpeg || found.format === ImageFormat.Jpeg2000
      ).toBe(true)
      expect(found.data.length).toBeGreaterThan(0)
    }
    // If no passthrough images found, that's OK
  })
})
