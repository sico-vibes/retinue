// Copies React Bits components (TypeScript + Tailwind variant) from their public registry into
// components/bits/, unchanged, with a header naming the source. Run: node scripts/add-bits.mjs Name...
// https://reactbits.dev/get-started/installation (registry @react-bits, variant -TS-TW).
import { mkdir, writeFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..', 'components', 'bits')
await mkdir(root, { recursive: true })

for (const name of process.argv.slice(2)) {
  const url = `https://reactbits.dev/r/${name}-TS-TW.json`
  const response = await fetch(url)
  if (!response.ok) { console.error(`${name}: ${response.status}`); process.exitCode = 1; continue }
  const item = await response.json()
  for (const file of item.files ?? []) {
    const base = file.path.split('/').pop()
    const header = `// From React Bits (https://reactbits.dev), ${name}, TS + Tailwind. Copied unchanged except where marked "Retinue:".\n'use client'\n\n`
    const content = file.content.startsWith("'use client'") || file.content.startsWith('"use client"') ? file.content : header + file.content
    await writeFile(join(root, base), content.startsWith('// From React Bits') ? content : `// From React Bits (https://reactbits.dev), ${name}, TS + Tailwind.\n` + content)
    console.log(`${name}: ${base}${item.dependencies?.length ? `  needs ${item.dependencies.join(', ')}` : ''}`)
  }
}
