import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const projectRoot = resolve(scriptDirectory, '..')
const sourcePath = resolve(process.env.MOVA_API_DOC_PATH ?? resolve(projectRoot, '../../docs/API.md'))
const serverRoutesPath = resolve(
  process.env.MOVA_SERVER_ROUTES_PATH ?? resolve(projectRoot, '../mova-server/src/routes'),
)
const websiteDataPath = resolve(projectRoot, 'src/data/apiDocs.ts')

for (const [label, path, environmentVariable] of [
  ['API source document', sourcePath, 'MOVA_API_DOC_PATH'],
  ['server routes directory', serverRoutesPath, 'MOVA_SERVER_ROUTES_PATH'],
]) {
  if (existsSync(path)) {
    continue
  }

  console.error(`${label} not found: ${path}`)
  console.error(`Set ${environmentVariable} to the current path and retry.`)
  process.exit(1)
}

const sourceDocument = readFileSync(sourcePath, 'utf8')
const websiteData = readFileSync(websiteDataPath, 'utf8')

const extractRouteCalls = (source) => {
  const calls = []
  let searchOffset = 0

  while (true) {
    const routeStart = source.indexOf('.route(', searchOffset)
    if (routeStart === -1) {
      return calls
    }

    const openingParenthesis = routeStart + '.route'.length
    let depth = 0
    let quote = null
    let escaped = false

    for (let index = openingParenthesis; index < source.length; index += 1) {
      const character = source[index]

      if (quote !== null) {
        if (escaped) {
          escaped = false
        } else if (character === '\\') {
          escaped = true
        } else if (character === quote) {
          quote = null
        }
        continue
      }

      if (character === '"' || character === "'") {
        quote = character
      } else if (character === '(') {
        depth += 1
      } else if (character === ')') {
        depth -= 1
        if (depth === 0) {
          calls.push(source.slice(routeStart, index + 1))
          searchOffset = index + 1
          break
        }
      }
    }

    if (searchOffset <= routeStart) {
      throw new Error(`Unterminated route declaration near byte ${routeStart}`)
    }
  }
}

const serverEndpoints = new Set()
for (const fileName of readdirSync(serverRoutesPath).filter(
  (fileName) => fileName.endsWith('.rs') && fileName !== 'mod.rs',
)) {
  const routeSource = readFileSync(join(serverRoutesPath, fileName), 'utf8')
  for (const routeCall of extractRouteCalls(routeSource)) {
    const path = routeCall.match(/\.route\(\s*"([^"]+)"/)?.[1]
    if (!path) {
      throw new Error(`Route path could not be parsed in ${fileName}: ${routeCall}`)
    }

    const methods = [
      ...routeCall.matchAll(
        /(?:(?:axum::routing::)|\.|\b)(get|post|put|patch|delete|head)\s*\(/g,
      ),
    ].map((match) => match[1].toUpperCase())
    if (!methods.length) {
      throw new Error(`Route methods could not be parsed for ${path} in ${fileName}`)
    }

    for (const method of methods) {
      serverEndpoints.add(`${method} /api${path}`)
    }
  }
}

const duplicateValues = (values) =>
  [...new Set(values.filter((value, index) => values.indexOf(value) !== index))].sort()

const sourceEndpointRows = [
  ...sourceDocument.matchAll(/\| `([A-Z]+)` \| `([^`]+)` \|/g),
].map((match) => `${match[1]} ${match[2]}`)
const sourceEndpointSections = [
  ...sourceDocument.matchAll(/^### `([A-Z]+) (\/api\/[^`]+)`$/gm),
].map((match) => `${match[1]} ${match[2]}`)
const websiteEndpointEntries = [
  ...websiteData.matchAll(/method: '([A-Z]+)', path: '([^']+)'/g),
].map((match) => `${match[1]} ${match[2]}`)

const sourceEndpoints = new Set(sourceEndpointRows)
const sourceSections = new Set(sourceEndpointSections)
const websiteEndpoints = new Set(websiteEndpointEntries)

const missingOnWebsite = [...sourceEndpoints].filter((endpoint) => !websiteEndpoints.has(endpoint)).sort()
const extraOnWebsite = [...websiteEndpoints].filter((endpoint) => !sourceEndpoints.has(endpoint)).sort()
const missingInSourceDocument = [...serverEndpoints]
  .filter((endpoint) => !sourceEndpoints.has(endpoint))
  .sort()
const missingOnServer = [...sourceEndpoints].filter((endpoint) => !serverEndpoints.has(endpoint)).sort()
const missingDetailedSections = [...sourceEndpoints]
  .filter((endpoint) => !sourceSections.has(endpoint))
  .sort()
const undocumentedDetailedSections = [...sourceSections]
  .filter((endpoint) => !sourceEndpoints.has(endpoint))
  .sort()
const duplicateSourceRows = duplicateValues(sourceEndpointRows)
const duplicateSourceSections = duplicateValues(sourceEndpointSections)
const duplicateWebsiteEntries = duplicateValues(websiteEndpointEntries)

if (
  missingOnWebsite.length ||
  extraOnWebsite.length ||
  missingInSourceDocument.length ||
  missingOnServer.length ||
  missingDetailedSections.length ||
  undocumentedDetailedSections.length ||
  duplicateSourceRows.length ||
  duplicateSourceSections.length ||
  duplicateWebsiteEntries.length
) {
  console.error('API documentation is not synchronized.')

  if (missingOnWebsite.length) {
    console.error('\nMissing on website:')
    missingOnWebsite.forEach((endpoint) => console.error(`- ${endpoint}`))
  }

  if (extraOnWebsite.length) {
    console.error('\nOnly on website:')
    extraOnWebsite.forEach((endpoint) => console.error(`- ${endpoint}`))
  }

  if (missingInSourceDocument.length) {
    console.error('\nImplemented by the server but missing from docs/API.md:')
    missingInSourceDocument.forEach((endpoint) => console.error(`- ${endpoint}`))
  }

  if (missingOnServer.length) {
    console.error('\nDocumented in docs/API.md but not implemented by the server:')
    missingOnServer.forEach((endpoint) => console.error(`- ${endpoint}`))
  }

  if (missingDetailedSections.length) {
    console.error('\nListed in docs/API.md but missing a detailed endpoint section:')
    missingDetailedSections.forEach((endpoint) => console.error(`- ${endpoint}`))
  }

  if (undocumentedDetailedSections.length) {
    console.error('\nDetailed endpoint sections missing from the docs/API.md overview:')
    undocumentedDetailedSections.forEach((endpoint) => console.error(`- ${endpoint}`))
  }

  for (const [label, duplicates] of [
    ['Duplicate docs/API.md overview entries', duplicateSourceRows],
    ['Duplicate docs/API.md detailed sections', duplicateSourceSections],
    ['Duplicate website endpoint entries', duplicateWebsiteEntries],
  ]) {
    if (duplicates.length) {
      console.error(`\n${label}:`)
      duplicates.forEach((endpoint) => console.error(`- ${endpoint}`))
    }
  }

  process.exit(1)
}

console.log(
  `API documentation synchronized: ${sourceEndpoints.size} endpoints match the server, overview, detailed sections, and website`,
)
