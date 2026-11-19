// Icons a bot may write in a reply as `:lucide-name:` (owner, 14 November 2026:
// tables and lists can carry a small icon when the employee's reply style allows
// it). A curated set, imported statically; any other Lucide name loads lazily (below).
import { useEffect, useState } from 'react'
import { AlertTriangle, ArrowDown, ArrowRight, ArrowUp, Ban, Bell, Bot, Bug, Calendar, Check, CheckCircle2, CircleDot, Clock, Cloud, Code, Database, DollarSign, Download, ExternalLink, Eye, File, FileText, Flag, Folder, GitBranch, GitPullRequest, Globe, Hammer, Heart, HelpCircle, Image, Info, Key, Lightbulb, Link, Lock, Mail, MessageSquare, Minus, Monitor, Package, Pause, Pencil, Play, Plus, Rocket, Search, Server, Settings, Shield, Sparkles, Star, Target, Terminal, ThumbsDown, ThumbsUp, Timer, Trash2, TrendingDown, TrendingUp, Upload, User, Users, Wrench, X, XCircle, Zap, type LucideIcon } from 'lucide-react'

export const REPLY_ICONS: Record<string, LucideIcon> = {
  'alert-triangle': AlertTriangle, 'triangle-alert': AlertTriangle, warning: AlertTriangle, 'arrow-down': ArrowDown, 'arrow-right': ArrowRight, 'arrow-up': ArrowUp, ban: Ban, bell: Bell, bot: Bot, bug: Bug, calendar: Calendar,
  check: Check, 'check-circle': CheckCircle2, 'circle-check': CheckCircle2, 'circle-dot': CircleDot, clock: Clock, cloud: Cloud, code: Code, database: Database, 'dollar-sign': DollarSign, download: Download, 'external-link': ExternalLink, eye: Eye,
  file: File, 'file-text': FileText, flag: Flag, folder: Folder, 'git-branch': GitBranch, 'git-pull-request': GitPullRequest, globe: Globe, hammer: Hammer, heart: Heart, 'help-circle': HelpCircle, 'circle-help': HelpCircle, image: Image, info: Info,
  key: Key, lightbulb: Lightbulb, link: Link, lock: Lock, mail: Mail, 'message-square': MessageSquare, minus: Minus, monitor: Monitor, package: Package, pause: Pause, pencil: Pencil, play: Play, plus: Plus, rocket: Rocket, search: Search,
  server: Server, settings: Settings, shield: Shield, sparkles: Sparkles, star: Star, target: Target, terminal: Terminal, 'thumbs-down': ThumbsDown, 'thumbs-up': ThumbsUp, timer: Timer, trash: Trash2, 'trash-2': Trash2,
  'trending-down': TrendingDown, 'trending-up': TrendingUp, upload: Upload, user: User, users: Users, wrench: Wrench, x: X, 'x-circle': XCircle, 'circle-x': XCircle, zap: Zap,
}

/** Tone by meaning, so a tick reads as done and a cross as a problem without the bot choosing colours. */
const TONE: Record<string, string> = { check: 'ok', 'check-circle': 'ok', 'circle-check': 'ok', 'thumbs-up': 'ok', 'trending-up': 'ok', x: 'bad', 'x-circle': 'bad', 'circle-x': 'bad', ban: 'bad', 'thumbs-down': 'bad', 'trending-down': 'bad', bug: 'bad', 'alert-triangle': 'warn', 'triangle-alert': 'warn', warning: 'warn', clock: 'warn', timer: 'warn' }

/** `crown` → `Crown`, `alert-triangle` → `AlertTriangle`: lucide-react's own export names. */
export function lucideExportName(name: string) {
  return name.split('-').filter(Boolean).map((part) => part[0]!.toUpperCase() + part.slice(1)).join('')
}

// Any other Lucide name (owner, 15 November 2026: `:lucide-crown:` and `:lucide-palette:` showed as text in a
// table) comes from the full icon set, loaded once and only when a reply first uses a name outside the set above.
// It is imported by its own path so Vite puts it in a separate chunk; importing `icons` from 'lucide-react' would
// pull every icon into the main bundle, because the package is already imported statically everywhere.
let fullSet: Promise<Record<string, LucideIcon>> | null = null
const resolved = new Map<string, LucideIcon | null>()
function loadIcon(name: string): Promise<LucideIcon | null> {
  fullSet ??= import('lucide-react/dist/esm/icons/index.js').then((module) => module as unknown as Record<string, LucideIcon>).catch(() => ({}))
  return fullSet.then((icons) => icons[lucideExportName(name)] ?? null).catch(() => null).then((icon) => { resolved.set(name, icon); return icon })
}

export function ReplyIcon({ name }: { name: string }) {
  const known = REPLY_ICONS[name] ?? resolved.get(name)
  const [loaded, setLoaded] = useState<LucideIcon | null | undefined>(known)
  useEffect(() => {
    if (known !== undefined) return
    let live = true
    void loadIcon(name).then((icon) => { if (live) setLoaded(icon) })
    return () => { live = false }
  }, [name, known])
  const Icon = known ?? loaded
  // While the set loads, hold the icon's room so the row doesn't jump; a name Lucide doesn't have stays plain text.
  if (Icon === undefined) return <span className="reply-icon reply-icon-pending" aria-hidden="true" />
  if (Icon === null) return <span>{`:lucide-${name}:`}</span>
  return <Icon className={`reply-icon${TONE[name] ? ` ${TONE[name]}` : ''}`} size={14} strokeWidth={2} role="img" aria-label={name.replace(/-/g, ' ')} />
}
