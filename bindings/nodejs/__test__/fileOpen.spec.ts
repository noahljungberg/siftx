import { describe, it, expect } from 'vitest'
import { SiftFile, FileType } from '..'
import { EXIFTOOL_IMAGES, POPPLER_TEST, hasExifToolImages, hasPopplerTest, findFirst, listFilesRecursive } from './helpers'
import { join } from 'node:path'

describe('file open', () => {
  it.skipIf(!hasExifToolImages())('opens JPEG and detects type', () => {
    const file = SiftFile.open(join(EXIFTOOL_IMAGES, 'Canon.jpg'))
    expect(file.fileType).toBe(FileType.Jpeg)
    file.close()
  })

  it.skipIf(!hasExifToolImages())('opens PNG and detects type', () => {
    const path = findFirst(EXIFTOOL_IMAGES, '.png')
    if (!path) return
    const file = SiftFile.open(path)
    expect(file.fileType).toBe(FileType.Png)
    file.close()
  })

  it.skipIf(!hasExifToolImages())('opens GIF and detects type', () => {
    const path = findFirst(EXIFTOOL_IMAGES, '.gif')
    if (!path) return
    const file = SiftFile.open(path)
    expect(file.fileType).toBe(FileType.Gif)
    file.close()
  })

  it.skipIf(!hasExifToolImages())('opens TIFF and detects type', () => {
    const path = findFirst(EXIFTOOL_IMAGES, '.tif') ?? findFirst(EXIFTOOL_IMAGES, '.tiff')
    if (!path) return
    const file = SiftFile.open(path)
    expect(file.fileType).toBe(FileType.Tiff)
    file.close()
  })

  it.skipIf(!hasExifToolImages())('opens WebP and detects type', () => {
    const path = findFirst(EXIFTOOL_IMAGES, '.webp')
    if (!path) return
    const file = SiftFile.open(path)
    expect(file.fileType).toBe(FileType.WebP)
    file.close()
  })

  it.skipIf(!hasPopplerTest())('opens PDF and detects type', () => {
    const paths = listFilesRecursive(POPPLER_TEST, '.pdf')
    if (paths.length === 0) return
    const file = SiftFile.open(paths[0])
    expect(file.fileType).toBe(FileType.Pdf)
    file.close()
  })

  it('throws on nonexistent file', () => {
    expect(() => SiftFile.open('/nonexistent/file.jpg')).toThrow(/SiftIOError/)
  })

  it.skipIf(!hasExifToolImages())('open -> parse -> close lifecycle', () => {
    const file = SiftFile.open(join(EXIFTOOL_IMAGES, 'Canon.jpg'))
    const doc = file.parse()
    expect(doc.fileType).toBe(FileType.Jpeg)
    doc.close()
  })

  it.skipIf(!hasExifToolImages())('closed file throws', () => {
    const file = SiftFile.open(join(EXIFTOOL_IMAGES, 'Canon.jpg'))
    file.close()
    expect(() => file.fileType).toThrow(/closed/)
  })
})
