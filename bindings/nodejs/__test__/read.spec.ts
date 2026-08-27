import { describe, it, expect } from 'vitest'
import { read, SiftFile, FileType } from '..'
import { EXIFTOOL_IMAGES, POPPLER_TEST, hasExifToolImages, hasPopplerTest, listFilesRecursive } from './helpers'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

describe('read from buffer', () => {
  it.skipIf(!hasExifToolImages())('reads JPEG from buffer', () => {
    const data = readFileSync(join(EXIFTOOL_IMAGES, 'Canon.jpg'))
    const doc = read(data)
    expect(doc.fileType).toBe(FileType.Jpeg)
    const t = doc.tags()
    expect(t.length).toBeGreaterThan(0)
    expect(t.some(tag => tag.name === 'Make' && tag.value === 'Canon')).toBe(true)
    doc.close()
  })

  it.skipIf(!hasPopplerTest())('reads PDF from buffer', () => {
    const paths = listFilesRecursive(POPPLER_TEST, '.pdf')
    if (paths.length === 0) return

    const data = readFileSync(paths[0])
    const doc = read(data)
    expect(doc.fileType).toBe(FileType.Pdf)
    doc.close()
  })

  it.skipIf(!hasExifToolImages())('read tags match open tags', () => {
    const filePath = join(EXIFTOOL_IMAGES, 'Canon.jpg')
    const data = readFileSync(filePath)

    const docFromRead = read(data)
    const tagsFromRead = docFromRead.tags()
    docFromRead.close()

    const file = SiftFile.open(filePath)
    const docFromOpen = file.parse()
    const tagsFromOpen = docFromOpen.tags()
    docFromOpen.close()

    expect(tagsFromRead.length).toBe(tagsFromOpen.length)
    for (let i = 0; i < tagsFromOpen.length; i++) {
      expect(tagsFromRead[i]).toEqual(tagsFromOpen[i])
    }
  })

  it.skipIf(!hasExifToolImages())('reads from copied buffer', () => {
    const original = readFileSync(join(EXIFTOOL_IMAGES, 'Canon.jpg'))
    const copy = Buffer.alloc(original.length)
    original.copy(copy)

    const doc = read(copy)
    expect(doc.fileType).toBe(FileType.Jpeg)
    doc.close()
  })
})
