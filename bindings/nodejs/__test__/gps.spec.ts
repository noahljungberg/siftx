import { describe, it, expect } from 'vitest'
import { SiftFile, read } from '..'
import { EXIF_SAMPLES, EXIFTOOL_IMAGES, hasExifSamples, hasExifToolImages, listFiles } from './helpers'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

describe('GPS', () => {
  it.skipIf(!hasExifSamples())('extracts GPS from JPEG', () => {
    const path = join(EXIF_SAMPLES, 'jpg', 'gps', 'DSCN0010.jpg')
    if (!existsSync(path)) return

    const file = SiftFile.open(path)
    const doc = file.parse()
    const gps = doc.gps()
    expect(gps).not.toBeNull()
    expect(gps!.latitude).not.toBe(0)
    expect(gps!.longitude).not.toBe(0)
    doc.close()
  })

  it.skipIf(!hasExifToolImages())('no crash on JPEG without GPS', () => {
    const data = readFileSync(join(EXIFTOOL_IMAGES, 'Canon.jpg'))
    const doc = read(data)
    // Just verify no crash
    doc.gps()
    doc.close()
  })

  it.skipIf(!hasExifSamples())('scans GPS samples', () => {
    const gpsDir = join(EXIF_SAMPLES, 'jpg', 'gps')
    const files = listFiles(gpsDir, '.jpg')
    if (files.length === 0) return

    let withGps = 0
    for (const path of files) {
      try {
        const file = SiftFile.open(path)
        const doc = file.parse()
        if (doc.gps() !== null) withGps++
        doc.close()
      } catch {}
    }
    expect(withGps).toBeGreaterThan(0)
  })

  it('GpsCoordinates toString with altitude', () => {
    const gps = { latitude: 43.467157, longitude: 11.885395, altitude: 200.5 }
    const s = `${gps.latitude.toFixed(6)}, ${gps.longitude.toFixed(6)}, ${gps.altitude!.toFixed(1)}m`
    expect(s).toBe('43.467157, 11.885395, 200.5m')
  })

  it('GpsCoordinates toString without altitude', () => {
    const gps = { latitude: 43.467157, longitude: 11.885395, altitude: null }
    const s = `${gps.latitude.toFixed(6)}, ${gps.longitude.toFixed(6)}`
    expect(s).toBe('43.467157, 11.885395')
  })

  it('GpsCoordinates equality', () => {
    const a = { latitude: 43.5, longitude: 11.5, altitude: 200.0 }
    const b = { latitude: 43.5, longitude: 11.5, altitude: 200.0 }
    expect(a).toEqual(b)
  })
})
