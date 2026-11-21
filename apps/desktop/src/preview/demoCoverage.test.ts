import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
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

  it('accounts for each named UI RPC without treating a fixture namespace as coverage', () => {
    const methodName = /^[a-z][\w-]*(?:\.[a-z][\w-]*)+$/i
    const uiMethods = new Set<string>()
    for (const path of [...productionSources('ui'), ...productionSources('remote'), 'tauri.ts']) {
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
    const fixtures = new Set<string>()
    for (const path of ['preview/fixtureBridge.ts', 'preview/fixtureTasks.ts', 'preview/remoteFixture.ts', 'preview/fixtureSuggestions.ts']) {
      walk(source(path), (node) => {
        if (ts.isCaseClause(node) && ts.isStringLiteral(node.expression)) fixtures.add(node.expression.text)
        if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.EqualsEqualsEqualsToken
          && ts.isIdentifier(node.left) && node.left.text === 'method' && ts.isStringLiteralLike(node.right)) fixtures.add(node.right.text)
      })
    }
    expect([...uiMethods].filter((name) => !fixtures.has(name) && !Object.prototype.hasOwnProperty.call(undemoedRpcs, name)).sort()).toEqual([])
    expect(Object.keys(undemoedRpcs).filter((name) => fixtures.has(name) || !uiMethods.has(name)).sort()).toEqual([])
  }, 30_000)
})

