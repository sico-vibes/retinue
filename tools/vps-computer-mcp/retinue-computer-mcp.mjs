#!/usr/bin/env node
// Retinue VPS computer MCP bridge.
// Stdio JSON-RPC 2.0. Requires Node 20+ (global fetch).
//
// Env:
//   HARBOR_COMPUTER_URL   https://dashboard.synthet.me
//   HARBOR_COMPUTER_TOKEN  hrb_...
//
// Do not log the token.

import readline from 'node:readline'

const BASE = (process.env.HARBOR_COMPUTER_URL || 'https://dashboard.synthet.me').replace(/\/+$/, '')
const TOKEN = process.env.HARBOR_COMPUTER_TOKEN || ''
const API = `${BASE}/harbor/api/computer`

if (!TOKEN) {
  console.error('HARBOR_COMPUTER_TOKEN is required')
  process.exit(1)
}

async function call(path, options = {}) {
  const response = await fetch(`${API}${path}`, {
    ...options,
    headers: {
      Authorization: `Bearer ${TOKEN}`,
      'X-Harbor-Client': 'mcp',
      ...(options.body ? { 'Content-Type': 'application/json' } : {}),
      ...(options.headers || {}),
    },
    signal: AbortSignal.timeout(120000),
  })
  const text = await response.text()
  let payload = text
  try {
    payload = text ? JSON.parse(text) : {}
  } catch {
    // Keep text.
  }
  if (!response.ok) {
    const message = payload?.error?.message || text || response.statusText
    throw new Error(`HTTP ${response.status}: ${message}`)
  }
  return payload
}

const tools = [
  {
    name: 'computer_status',
    description: 'Read the bot computer state, CPU/memory usage, port and persistent disk usage.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  },
  {
    name: 'computer_wake',
    description: 'Wake (start) the 24/7 bot computer. Use before work that needs the desktop, browser or a routine.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  },
  {
    name: 'computer_sleep',
    description: 'Stop the bot computer while keeping its persistent /config volume.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  },
  {
    name: 'computer_restart',
    description: 'Restart the bot computer without deleting its persistent /config volume.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  },
  {
    name: 'computer_logs',
    description: 'Read the tail of the computer container log.',
    inputSchema: {
      type: 'object',
      properties: { tail: { type: 'number', description: 'Lines from the end, default 200, max 1000.' } },
      additionalProperties: false,
    },
  },
  {
    name: 'computer_exec',
    description: 'Run a shell command inside the bot computer as the desktop user, in /config. Use this to install CLIs, run scripts, or drive automation. The command runs inside the container, never on the VPS host.',
    inputSchema: {
      type: 'object',
      properties: {
        command: { type: 'string', description: 'Shell command to run inside the computer.' },
        timeout_ms: { type: 'number', description: 'Timeout in milliseconds, max 300000.' },
      },
      required: ['command'],
      additionalProperties: false,
    },
  },
  {
    name: 'computer_list_routines',
    description: 'List schedules stored on the computer.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  },
  {
    name: 'computer_create_routine',
    description: 'Create a cron routine that runs a shell command inside the computer. Routines run only while the computer is awake.',
    inputSchema: {
      type: 'object',
      properties: {
        name: { type: 'string' },
        cron: { type: 'string', description: 'Five-field cron in Europe/London, e.g. 0 8 * * *.' },
        command: { type: 'string' },
      },
      required: ['name', 'cron', 'command'],
      additionalProperties: false,
    },
  },
  {
    name: 'computer_run_routine',
    description: 'Run an existing routine immediately.',
    inputSchema: {
      type: 'object',
      properties: { id: { type: 'string' } },
      required: ['id'],
      additionalProperties: false,
    },
  },
  {
    name: 'computer_desktop_url',
    description: 'Return the owner-only live desktop URL. Opening it requires a Harbor owner login.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  },
]

function text(value) {
  return {
    content: [
      {
        type: 'text',
        text: typeof value === 'string' ? value : JSON.stringify(value, null, 2),
      },
    ],
  }
}

async function runTool(name, args = {}) {
  if (name === 'computer_status') {
    return text(await call(''))
  }
  if (name === 'computer_wake') {
    return text(await call('/action/wake', { method: 'POST', body: JSON.stringify({}) }))
  }
  if (name === 'computer_sleep') {
    return text(await call('/action/stop', { method: 'POST', body: JSON.stringify({}) }))
  }
  if (name === 'computer_restart') {
    return text(await call('/action/restart', { method: 'POST', body: JSON.stringify({}) }))
  }
  if (name === 'computer_logs') {
    const tail = Math.max(20, Math.min(1000, Number(args.tail) || 200))
    return text(await call(`/logs?tail=${tail}`))
  }
  if (name === 'computer_exec') {
    return text(await call('/exec', {
      method: 'POST',
      body: JSON.stringify({
        command: args.command,
        timeout_ms: Number(args.timeout_ms) || undefined,
      }),
    }))
  }
  if (name === 'computer_list_routines') {
    return text(await call('/routines'))
  }
  if (name === 'computer_create_routine') {
    return text(await call('/routines', {
      method: 'POST',
      body: JSON.stringify({ name: args.name, cron: args.cron, command: args.command }),
    }))
  }
  if (name === 'computer_run_routine') {
    return text(await call(`/routines/${encodeURIComponent(args.id)}/run`, {
      method: 'POST',
      body: JSON.stringify({}),
    }))
  }
  if (name === 'computer_desktop_url') {
    const status = await call('')
    return text({
      desktop_url: status.desktop_url,
      note: 'Owner-only live view. The Harbor owner must sign in.',
    })
  }
  throw new Error(`unknown tool ${name}`)
}

const rl = readline.createInterface({ input: process.stdin, terminal: false })

function send(id, result) {
  process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', id, result })}\n`)
}

function sendError(id, code, message) {
  process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', id, error: { code, message } })}\n`)
}

rl.on('line', async (line) => {
  let message
  try {
    message = JSON.parse(line)
  } catch {
    return
  }
  const { id, method, params } = message || {}
  if (method === 'notifications/initialized' || method === 'notifications/cancelled') {
    return
  }
  if (method === 'initialize') {
    send(id, {
      protocolVersion: '2024-11-05',
      capabilities: { tools: {} },
      serverInfo: { name: 'retinue-vps-computer', version: '1.0.0' },
    })
    return
  }
  if (method === 'ping') {
    send(id, {})
    return
  }
  if (method === 'tools/list') {
    send(id, { tools })
    return
  }
  if (method === 'tools/call') {
    try {
      const result = await runTool(params?.name, params?.arguments || {})
      send(id, result)
    } catch (error) {
      sendError(id, -32000, error.message)
    }
    return
  }
  if (method === 'shutdown') {
    send(id, {})
    return
  }
  if (method === 'exit') {
    process.exit(0)
  }
  sendError(id, -32601, `method not found: ${method}`)
})
