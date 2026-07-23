import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const projectRoot = resolve(scriptDirectory, '..')
const sourcePath = resolve(process.env.MOVA_API_DOC_PATH ?? resolve(projectRoot, '../mova/docs/API.md'))
const websiteDataPath = resolve(projectRoot, 'src/data/apiDocs.ts')

if (!existsSync(sourcePath)) {
  console.error(`API source document not found: ${sourcePath}`)
  console.error('Set MOVA_API_DOC_PATH to the current mova/docs/API.md path and retry.')
  process.exit(1)
}

const sourceDocument = readFileSync(sourcePath, 'utf8')
const websiteData = readFileSync(websiteDataPath, 'utf8')

const sourceEndpoints = new Set(
  [...sourceDocument.matchAll(/\| `([A-Z]+)` \| `([^`]+)` \|/g)].map((match) => `${match[1]} ${match[2]}`),
)
const websiteEndpoints = new Set(
  [...websiteData.matchAll(/method: '([A-Z]+)', path: '([^']+)'/g)].map((match) => `${match[1]} ${match[2]}`),
)

const missingOnWebsite = [...sourceEndpoints].filter((endpoint) => !websiteEndpoints.has(endpoint)).sort()
const extraOnWebsite = [...websiteEndpoints].filter((endpoint) => !sourceEndpoints.has(endpoint)).sort()

if (missingOnWebsite.length || extraOnWebsite.length) {
  console.error('API documentation is not synchronized.')

  if (missingOnWebsite.length) {
    console.error('\nMissing on website:')
    missingOnWebsite.forEach((endpoint) => console.error(`- ${endpoint}`))
  }

  if (extraOnWebsite.length) {
    console.error('\nOnly on website:')
    extraOnWebsite.forEach((endpoint) => console.error(`- ${endpoint}`))
  }

  process.exit(1)
}

console.log(`API documentation synchronized: ${sourceEndpoints.size} endpoints match ${sourcePath}`)
