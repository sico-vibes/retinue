// The Model Context Protocol mark, beside a conversation another program
// started through Retinue's MCP server (owner, 14 November 2026).
export function McpMark({ client, size = 12 }: { client?: string; size?: number }) {
  const label = client ? `Started by ${client} through the MCP` : 'Started through the MCP'
  return <span className="mcp-mark" role="img" aria-label={label} title={label}>
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M3.5 11.5 11 4a3 3 0 0 1 4.2 4.2L9.6 13.8" />
      <path d="M9.7 13.7 15.2 8.2a3 3 0 0 1 4.2 4.2l-6.6 6.6a.9.9 0 0 0 0 1.2l1.3 1.3" />
      <path d="M13.1 6.1 7.6 11.6a3 3 0 0 0 4.2 4.2l5.5-5.5" />
    </svg>
  </span>
}
