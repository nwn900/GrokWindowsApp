const fs = require('fs')
const path = require('path')

const ROOT_DIR = process.cwd()
const SKIP_DIRS = new Set(['.git', 'dist', 'node_modules'])
const TEXT_FILE_EXTENSIONS = new Set([
  '.dockerfile',
  '.js',
  '.json',
  '.md',
  '.svg',
  '.txt',
  '.yml',
  '.yaml'
])
const FORBIDDEN_PATTERNS = [
  ['c', 'l', 'a', 'u', 'd', 'e'],
  ['a', 'n', 't', 'h', 'r', 'o', 'p', 'i', 'c'],
  ['c', 'l', 'a', 'u', 'd', 'e', '.', 'a', 'i']
].map((parts) => new RegExp(parts.join(''), 'i'))

function shouldScan(filePath) {
  const ext = path.extname(filePath).toLowerCase()
  return TEXT_FILE_EXTENSIONS.has(ext)
}

function walk(dirPath, results) {
  for (const entry of fs.readdirSync(dirPath, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (SKIP_DIRS.has(entry.name)) {
        continue
      }

      walk(path.join(dirPath, entry.name), results)
      continue
    }

    const fullPath = path.join(dirPath, entry.name)

    if (!shouldScan(fullPath)) {
      continue
    }

    const relativePath = path.relative(ROOT_DIR, fullPath)
    const content = fs.readFileSync(fullPath, 'utf8')
    const lines = content.split(/\r?\n/)

    lines.forEach((line, index) => {
      if (FORBIDDEN_PATTERNS.some((pattern) => pattern.test(line))) {
        results.push(`${relativePath}:${index + 1}: ${line.trim()}`)
      }
    })
  }
}

const results = []
walk(ROOT_DIR, results)

if (results.length > 0) {
  console.error('Found stale legacy branding:')
  for (const result of results) {
    console.error(`- ${result}`)
  }
  process.exit(1)
}

console.log('Branding audit passed.')
