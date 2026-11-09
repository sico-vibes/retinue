// Serves the exported site in out/ exactly as a static host would.
import { createServer } from 'node:http'
import { readFile, stat } from 'node:fs/promises'
import { extname, join, normalize, resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { gzipSync } from 'node:zlib'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../out')
const port = Number(process.env.PORT ?? 5291)
const types = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.webp': 'image/webp', '.png': 'image/png', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.ico': 'image/x-icon', '.txt': 'text/plain' }

createServer(async (request, response) => {
  const path = normalize(decodeURIComponent(new URL(request.url, 'http://x').pathname)).replace(/^([/\\])+/, '')
  let file = join(root, path)
  if (!file.startsWith(root)) { response.writeHead(403).end(); return }
  try {
    if ((await stat(file)).isDirectory()) file = join(file, 'index.html')
    const body = await readFile(file)
    const immutable = file.includes('_next') || /\.(webp|woff2)$/.test(file)
    const headers = { 'Content-Type': types[extname(file)] ?? 'application/octet-stream', 'Cache-Control': immutable ? 'public, max-age=31536000, immutable' : 'no-cache' }
    // Text is compressed, as any real host would.
    if (/\.(html|js|css|json|svg|txt)$/.test(file)) response.writeHead(200, { ...headers, 'Content-Encoding': 'gzip' }).end(gzipSync(body))
    else response.writeHead(200, headers).end(body)
  } catch {
    const missing = await readFile(join(root, '404.html')).catch(() => 'Not found')
    response.writeHead(404, { 'Content-Type': types['.html'] }).end(missing)
  }
}).listen(port, '127.0.0.1', () => console.log(`http://127.0.0.1:${port}/`))
