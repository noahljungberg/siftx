import { existsSync, readdirSync, statSync } from 'node:fs'
import { join, resolve } from 'node:path'

function findRepoRoot(): string {
  let dir = resolve(__dirname, '..', '..', '..')
  // Walk up until we find Cargo.toml
  while (dir !== '/') {
    if (existsSync(join(dir, 'Cargo.toml'))) return dir
    dir = resolve(dir, '..')
  }
  return resolve(__dirname, '..', '..', '..')
}

const REPO_ROOT = findRepoRoot()

export const EXIFTOOL_IMAGES = join(REPO_ROOT, 'testdata', 'exiftool-images')
export const EXIF_SAMPLES = join(REPO_ROOT, 'testdata', 'exif-samples')
export const POPPLER_TEST = join(REPO_ROOT, 'testdata', 'poppler-test')

export function hasExifToolImages(): boolean {
  return existsSync(EXIFTOOL_IMAGES) && statSync(EXIFTOOL_IMAGES).isDirectory()
}

export function hasExifSamples(): boolean {
  return existsSync(EXIF_SAMPLES) && statSync(EXIF_SAMPLES).isDirectory()
}

export function hasPopplerTest(): boolean {
  return existsSync(POPPLER_TEST) && statSync(POPPLER_TEST).isDirectory()
}

export function findFirst(dir: string, ext: string): string | null {
  if (!existsSync(dir)) return null
  const files = readdirSync(dir).filter(f => f.endsWith(ext))
  return files.length > 0 ? join(dir, files[0]) : null
}

export function listFiles(dir: string, ext: string): string[] {
  if (!existsSync(dir)) return []
  return readdirSync(dir).filter(f => f.endsWith(ext)).map(f => join(dir, f))
}

export function listFilesRecursive(dir: string, ext: string): string[] {
  const results: string[] = []
  function walk(d: string) {
    if (!existsSync(d)) return
    for (const entry of readdirSync(d, { withFileTypes: true })) {
      const full = join(d, entry.name)
      if (entry.isDirectory()) {
        walk(full)
      } else if (entry.name.endsWith(ext)) {
        results.push(full)
      }
    }
  }
  walk(dir)
  return results
}
