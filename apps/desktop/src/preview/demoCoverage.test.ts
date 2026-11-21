import { workflowRpcErrors, pairingFailures, clipboardReadFailures, dictationReasons } from './demoWorkflowErrors'
import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import ts from 'typescript'
import { demoEntries } from './demoRegistry'
import { undemoedRpcs } from './demoCoverageAllowlist'

const root = fileURLToPath(new URL('..', import.meta.url))
const read = (path: string) => readFileSync(join(root, path), 'utf8')
const source = (path: string, text = read(path)) => ts.createSourceFile(path, text, ts.ScriptTarget.Latest)

function walk(node: ts.Node, visit: (node: ts.Node) => void) {
  visit(node)
  node.forEachChild((child) => walk(child, visit))
}

function productionSources(dir: string): string[] {
  return readdirSync(join(root, dir), { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) return productionSources(path)
    return /\.tsx?$/.test(entry.name) && !/\.test\.tsx?$/.test(entry.name) ? [path] : []
  })
}

describe('preview demo coverage', () => {
  it('registers every code recognized by production error copy', () => {
    const codes = new Set<string>()
    walk(source('ui/errorCopy.ts'), (node) => {
      if (ts.isCaseClause(node) && ts.isStringLiteral(node.expression)) codes.add(node.expression.text)
    })
    const registered = demoEntries.flatMap((entry) => entry.errorCode ? [entry.errorCode] : [])
    expect(registered.sort()).toEqual([...codes].sort())
    expect(new Set(demoEntries.map((entry) => entry.id)).size).toBe(demoEntries.length)
  })

  it('links representative production fallback literals to callable demo routes', () => {
    const cases: Record<string, Record<string, string>> = {
      'ui/DiagnosticsDialog.tsx': {
        'The diagnostics list could not be read.': 'diagnostics-list-error',
        'The save dialog could not open.': 'diagnostics-dialog-error',
        'The diagnostics file could not be saved.': 'diagnostics-write-error',
      },
      'ui/BackupPage.tsx': {
        'The save dialog could not open.': 'backup-save-dialog-error',
        'The template could not be saved.': 'template-write-error',
        'The backup could not be saved.': 'backup-write-error',
        'The file dialog could not open.': 'backup-file-dialog-error',
        'That file could not be read as a Retinue backup.': 'backup-read-error',
        'The backup could not be restored.': 'backup-restore-error',
      },
      'ui/ApprovalRulesPage.tsx': {
        'Saved approvals could not be read.': 'approvals-list-error',
        'That approval could not be removed. Try again.': 'approvals-delete-error',
        'That approval could not be changed. Try again.': 'approvals-update-error',
      },
      'remote/PushCard.tsx': {
        'Notifications could not be turned on here.': 'push-subscribe-error',
        'Notifications could not be turned off here.': 'push-unsubscribe-error',
      },
    }
    for (const [path, covered] of Object.entries(cases)) {
      const literals = new Set<string>()
      walk(source(path), (node) => {
        if (ts.isStringLiteralLike(node) && /could not be |could not open/.test(node.text)) literals.add(node.text)
      })
      expect([...literals].sort(), path).toEqual(Object.keys(covered).sort())
      for (const id of Object.values(covered)) {
        const entry = demoEntries.find((item) => item.id === id)
        expect(entry, id).toBeDefined()
        expect(entry?.url, id).toMatch(/[?&]demo-(?:rpc-error|diagnostics|backup)=/)
      }
    }
  })

  it('registers the audited workflow RPC codes and pairing job reasons', () => {
    const daemon = fileURLToPath(new URL('../../../../crates/retinue-daemon/src/', import.meta.url))
    const contracts = {
      'machine.probe': 'machine_client.rs',
      'machine.pair.begin': 'machines.rs',
      'project.clone': 'project_clone.rs',
      'project.discover': 'project_clone.rs',
      'project.content.search': 'project_search.rs',
      'project.sharedSettings.trust': 'shared_settings.rs',
      'project.sharedSettings.untrust': 'shared_settings.rs',
      'project.sharedSettings.write': 'shared_settings.rs',
      'project.scaffold': 'scaffold.rs',
      'project.create': 'main.rs',
      'project.update': 'main.rs',
      'project.delete': 'main.rs',
    } satisfies Record<keyof typeof workflowRpcErrors, string>
    const storageErrors = readFileSync(join(daemon, 'main.rs'), 'utf8').split('fn agent_error(')[1]?.split('fn derr(')[0] ?? ''
    for (const method of Object.keys(contracts) as Array<keyof typeof contracts>) {
      const handler = readFileSync(join(daemon, contracts[method]), 'utf8')
      for (const [code, message] of Object.entries(workflowRpcErrors[method])) {
        expect(method.startsWith('project.sharedSettings.') ? handler + storageErrors : handler, method).toContain(message)
        const entry = demoEntries.find((item) => item.id === `workflow-${method}-${code}`)
        const params = new URL(entry?.url ?? '/', 'http://preview.test').searchParams
        expect(params.get('demo-rpc-error')).toBe(method)
        expect(params.get('demo-error-code')).toBe(code)
        expect(entry?.steps?.length).toBeGreaterThan(0)
      }
    }
    for (const reason of Object.keys(pairingFailures)) {
      expect(demoEntries.find((entry) => entry.id === `machine-pair-${reason}`)?.url).toContain(`demo-machine-pair=${reason}`)
    }
  })

  it('covers the native clipboard read errors that the composer displays', () => {
    const native = fileURLToPath(new URL('../../src-tauri/src/clipboard.rs', import.meta.url))
    const messages = [...new Set(readFileSync(native, 'utf8').match(/The (?:system )?clipboard[^"\n]+\./g))]
      .filter((message) => message !== 'The clipboard does not contain a readable image.')
    expect(messages.sort()).toEqual(Object.values(clipboardReadFailures).sort())
    for (const [id, message] of Object.entries(clipboardReadFailures)) {
      expect(demoEntries.find((entry) => entry.id === `clipboard-image-${id}-error`)).toMatchObject({ label: message, steps: [{ paste: '.composer-box textarea' }] })
    }
  })

  it('covers each dictation reason on both production island variants', () => {
    const reasons: string[] = []
    walk(source('ui/dictation.ts'), (node) => {
      if (!ts.isVariableDeclaration(node) || !ts.isIdentifier(node.name) || node.name.text !== 'CLOUD_REASON_COPY' || !node.initializer || !ts.isObjectLiteralExpression(node.initializer)) return
      for (const property of node.initializer.properties) {
        if (ts.isPropertyAssignment(property) && (ts.isStringLiteral(property.name) || ts.isIdentifier(property.name))) reasons.push(property.name.text)
      }
    })
    expect(dictationReasons).toEqual(reasons)
    const previews = demoEntries.map((entry) => new URL(entry.url, 'http://preview.test').searchParams)
    for (const reason of reasons) for (const surface of ['bubble', 'companion']) for (const fallback of ['missing', 'failed']) {
      expect(previews.some((params) => params.get('reason') === reason && params.get('surface') === surface && params.get('fallback') === fallback)).toBe(true)
    }
  })

  const paths = [...productionSources('ui'), ...productionSources('remote'), 'tauri.ts']
  const fixtureMethods = new Set<string>()
  const fixturePaths = ['preview/fixtureBridge.ts', 'preview/fixtureTasks.ts', 'preview/remoteFixture.ts', 'preview/fixtureSuggestions.ts']
  const methodName = /^[a-z][\w-]*(?:\.[a-z][\w-]*)+$/i
  const knownUiMethods = new Set<string>()

  beforeAll(() => {
    for (const path of fixturePaths) {
      walk(source(path), (node) => {
        if (ts.isCaseClause(node) && ts.isStringLiteral(node.expression)) fixtureMethods.add(node.expression.text)
        if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.EqualsEqualsEqualsToken
          && ts.isIdentifier(node.left) && node.left.text === 'method' && ts.isStringLiteralLike(node.right)) fixtureMethods.add(node.right.text)
      })
    }
  })

  // Keep the source scan within the UI gate's per-test limit on the shared build machine.
  const batchSize = 40
  const batches = Array.from({ length: Math.ceil(paths.length / batchSize) }, (_, i) => ({
    batch: i + 1, files: paths.slice(i * batchSize, (i + 1) * batchSize),
  }))
  it.each(batches)('requires literal fixture branches for named UI RPCs (batch $batch)', ({ files }) => {
    const uiMethods = new Set<string>()
    for (const path of files) {
      const text = read(path)
      if (!/\b(?:rpc|doRpc|visibleRpc|call)\s*[<(]/i.test(text)) continue
      walk(source(path, text), (node) => {
        if (ts.isCallExpression(node)) {
          const arg = node.arguments[0]
          const name = ts.isIdentifier(node.expression) ? node.expression.text : ts.isPropertyAccessExpression(node.expression) ? node.expression.name.text : ''
          if (arg && ts.isStringLiteralLike(arg) && /^(rpc|doRpc|visibleRpc|call)$/i.test(name) && methodName.test(arg.text)) uiMethods.add(arg.text)
        }
      })
    }
    for (const name of uiMethods) knownUiMethods.add(name)
    expect([...uiMethods].filter((name) => !fixtureMethods.has(name)).sort()).toEqual([])
  })

  afterAll(() => {
    for (const [name, reason] of Object.entries(undemoedRpcs)) {
      expect(knownUiMethods.has(name), name).toBe(true)
      expect(reason.trim(), name).not.toBe('')
      expect(demoEntries.some((entry) => new URL(entry.url, 'http://preview.test').searchParams.get('demo-rpc-error') === name), name).toBe(true)
    }
  })
})
