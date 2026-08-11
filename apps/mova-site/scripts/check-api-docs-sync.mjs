import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const projectRoot = resolve(scriptDirectory, '..')
const sourcePath = resolve(process.env.MOVA_API_DOC_PATH ?? resolve(projectRoot, '../../docs/API.md'))
const serverRoutesPath = resolve(
  process.env.MOVA_SERVER_ROUTES_PATH ?? resolve(projectRoot, '../mova-server/src/routes'),
)
const serverHandlersPath = resolve(
  process.env.MOVA_SERVER_HANDLERS_PATH ?? join(dirname(serverRoutesPath), 'handlers'),
)
const websiteDataPath = resolve(projectRoot, 'src/data/apiDocs.ts')

for (const [label, path, environmentVariable] of [
  ['API source document', sourcePath, 'MOVA_API_DOC_PATH'],
  ['server routes directory', serverRoutesPath, 'MOVA_SERVER_ROUTES_PATH'],
  ['server handlers directory', serverHandlersPath, 'MOVA_SERVER_HANDLERS_PATH'],
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
const serverRouteHandlers = []
for (const fileName of readdirSync(serverRoutesPath).filter(
  (fileName) => fileName.endsWith('.rs') && fileName !== 'mod.rs',
)) {
  const routeSource = readFileSync(join(serverRoutesPath, fileName), 'utf8')
  for (const routeCall of extractRouteCalls(routeSource)) {
    const path = routeCall.match(/\.route\(\s*"([^"]+)"/)?.[1]
    if (!path) {
      throw new Error(`Route path could not be parsed in ${fileName}: ${routeCall}`)
    }

    const handlerBindings = [
      ...routeCall.matchAll(
        /(?:(?:axum::routing::)|\.|\b)(get|post|put|patch|delete|head)\s*\(\s*handlers::([a-z0-9_]+)::([a-z0-9_]+)\s*\)/g,
      ),
    ]
    if (!handlerBindings.length) {
      throw new Error(`Route methods and handlers could not be parsed for ${path} in ${fileName}`)
    }

    for (const binding of handlerBindings) {
      const endpoint = `${binding[1].toUpperCase()} /api${path}`
      serverEndpoints.add(endpoint)
      serverRouteHandlers.push({
        endpoint,
        moduleName: binding[2],
        handlerName: binding[3],
      })
    }
  }
}

const extractHandlerParameters = (source, handlerName) => {
  const declaration = `pub async fn ${handlerName}`
  const declarationOffset = source.indexOf(declaration)
  if (declarationOffset === -1) {
    throw new Error(`Handler declaration not found: ${handlerName}`)
  }
  const openingParenthesis = source.indexOf('(', declarationOffset + declaration.length)
  if (openingParenthesis === -1) {
    throw new Error(`Handler parameters could not be parsed: ${handlerName}`)
  }

  let depth = 0
  for (let index = openingParenthesis; index < source.length; index += 1) {
    if (source[index] === '(') {
      depth += 1
    } else if (source[index] === ')') {
      depth -= 1
      if (depth === 0) {
        return source.slice(openingParenthesis + 1, index)
      }
    }
  }

  throw new Error(`Unterminated handler parameters: ${handlerName}`)
}

const handlerParameterCache = new Map()
const routeHandlerParameters = new Map(
  serverRouteHandlers.map((binding) => {
    const cacheKey = `${binding.moduleName}::${binding.handlerName}`
    if (!handlerParameterCache.has(cacheKey)) {
      const handlerPath = join(serverHandlersPath, `${binding.moduleName}.rs`)
      if (!existsSync(handlerPath)) {
        throw new Error(`Handler source not found: ${handlerPath}`)
      }
      const handlerSource = readFileSync(handlerPath, 'utf8')
      handlerParameterCache.set(
        cacheKey,
        extractHandlerParameters(handlerSource, binding.handlerName),
      )
    }
    return [binding.endpoint, handlerParameterCache.get(cacheKey)]
  }),
)

const duplicateValues = (values) =>
  [...new Set(values.filter((value, index) => values.indexOf(value) !== index))].sort()

const sourceEndpointRowMatches = [
  ...sourceDocument.matchAll(/\| `([A-Z]+)` \| `([^`]+)` \| ([^|\n]+) \|/g),
]
const sourceEndpointRows = sourceEndpointRowMatches.map((match) => `${match[1]} ${match[2]}`)
const sourceEndpointSectionMatches = [
  ...sourceDocument.matchAll(/^### `([A-Z]+) (\/api\/[^`]+)`$/gm),
]
const sourceEndpointSections = sourceEndpointSectionMatches.map(
  (match) => `${match[1]} ${match[2]}`,
)
const websiteEndpointMatches = [
  ...websiteData.matchAll(/method: '([A-Z]+)', path: '([^']+)', description: '([^']+)'/g),
]
const websiteEndpointEntries = websiteEndpointMatches.map((match) => `${match[1]} ${match[2]}`)

const sourceEndpoints = new Set(sourceEndpointRows)
const sourceSections = new Set(sourceEndpointSections)
const websiteEndpoints = new Set(websiteEndpointEntries)
const sourceDescriptions = new Map(
  sourceEndpointRowMatches.map((match) => [`${match[1]} ${match[2]}`, match[3].trim()]),
)
const websiteDescriptions = new Map(
  websiteEndpointMatches.map((match) => [`${match[1]} ${match[2]}`, match[3].trim()]),
)
const sourceSectionBodies = new Map(
  sourceEndpointSectionMatches.map((match, index) => {
    const start = match.index + match[0].length
    const end = sourceEndpointSectionMatches[index + 1]?.index ?? sourceDocument.length
    return [`${match[1]} ${match[2]}`, sourceDocument.slice(start, end)]
  }),
)
const sourceStreamErrors = new Set(
  [
    ...sourceDocument.matchAll(
      /\|\s*`(\d{3})`\s*\|\s*`((?:strm|remote)_[a-z0-9_]+)`\s*\|/g,
    ),
  ].map((match) => `${match[1]} ${match[2]}`),
)
const websiteStreamErrors = new Set(
  [
    ...websiteData.matchAll(
      /status:\s*'(\d{3})',\s*errorCode:\s*'((?:strm|remote)_[a-z0-9_]+)'/g,
    ),
  ].map((match) => `${match[1]} ${match[2]}`),
)

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
const missingStreamErrorsOnWebsite = [...sourceStreamErrors]
  .filter((error) => !websiteStreamErrors.has(error))
  .sort()
const extraStreamErrorsOnWebsite = [...websiteStreamErrors]
  .filter((error) => !sourceStreamErrors.has(error))
  .sort()
const mismatchedEndpointDescriptions = [...sourceDescriptions]
  .filter(([endpoint, description]) => websiteDescriptions.get(endpoint) !== description)
  .map(
    ([endpoint, description]) =>
      `${endpoint}: docs=${JSON.stringify(description)} website=${JSON.stringify(websiteDescriptions.get(endpoint))}`,
  )
  .sort()
const adminEndpointsMissingPermissionSections = [...sourceDescriptions]
  .filter(([, description]) => description.includes('（管理员）'))
  .filter(([endpoint]) => {
    const section = sourceSectionBodies.get(endpoint) ?? ''
    return !section.includes('权限：') || !/(?:\badmin\b|管理员)/.test(section)
  })
  .map(([endpoint]) => endpoint)
  .sort()
const anonymousEndpoints = new Set([
  'GET /api/health',
  'GET /api/auth/bootstrap-status',
  'POST /api/auth/bootstrap-admin',
  'POST /api/auth/login',
  'POST /api/auth/token-login',
  'POST /api/auth/refresh',
  'POST /api/auth/logout',
])
const protectedEndpointsMissingTypedAuthentication = [...serverEndpoints]
  .filter((endpoint) => !anonymousEndpoints.has(endpoint))
  .filter((endpoint) => {
    const parameters = routeHandlerParameters.get(endpoint) ?? ''
    return !/(?:AuthenticatedUser|AdminUser|AuthenticatedContext)/.test(parameters)
  })
  .sort()
const administratorEndpointsMissingAdminExtractor = [...sourceDescriptions]
  .filter(([, description]) => description.includes('（管理员）'))
  .filter(([endpoint]) => !(routeHandlerParameters.get(endpoint) ?? '').includes('AdminUser'))
  .map(([endpoint]) => endpoint)
  .sort()

if (
  missingOnWebsite.length ||
  extraOnWebsite.length ||
  missingInSourceDocument.length ||
  missingOnServer.length ||
  missingDetailedSections.length ||
  undocumentedDetailedSections.length ||
  duplicateSourceRows.length ||
  duplicateSourceSections.length ||
  duplicateWebsiteEntries.length ||
  missingStreamErrorsOnWebsite.length ||
  extraStreamErrorsOnWebsite.length ||
  mismatchedEndpointDescriptions.length ||
  adminEndpointsMissingPermissionSections.length ||
  protectedEndpointsMissingTypedAuthentication.length ||
  administratorEndpointsMissingAdminExtractor.length
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

  if (missingStreamErrorsOnWebsite.length) {
    console.error('\nSTRM playback status and error-code pairs missing on website:')
    missingStreamErrorsOnWebsite.forEach((error) => console.error(`- ${error}`))
  }

  if (extraStreamErrorsOnWebsite.length) {
    console.error('\nSTRM playback status and error-code pairs missing from docs/API.md:')
    extraStreamErrorsOnWebsite.forEach((error) => console.error(`- ${error}`))
  }

  if (mismatchedEndpointDescriptions.length) {
    console.error('\nEndpoint descriptions differ between docs/API.md and the website:')
    mismatchedEndpointDescriptions.forEach((description) => console.error(`- ${description}`))
  }

  if (adminEndpointsMissingPermissionSections.length) {
    console.error('\nAdministrator endpoints missing an explicit admin permission section:')
    adminEndpointsMissingPermissionSections.forEach((endpoint) => console.error(`- ${endpoint}`))
  }

  if (protectedEndpointsMissingTypedAuthentication.length) {
    console.error('\nProtected routes missing a typed authentication extractor:')
    protectedEndpointsMissingTypedAuthentication.forEach((endpoint) => console.error(`- ${endpoint}`))
  }

  if (administratorEndpointsMissingAdminExtractor.length) {
    console.error('\nAdministrator routes missing the AdminUser extractor:')
    administratorEndpointsMissingAdminExtractor.forEach((endpoint) => console.error(`- ${endpoint}`))
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
  `API documentation synchronized: ${sourceEndpoints.size} endpoints and descriptions match the server, overview, detailed sections, and website`,
)
