// The chat background: the layer behind a conversation, and its group in Settings › Appearance.
import { useRef, useState, type CSSProperties } from 'react'
import { ImagePlus } from 'lucide-react'
import { useThemeStyle } from '../blob/retinue'
import { CHAT_BACKGROUND_OPACITY_MAX, CHAT_BACKGROUND_OPACITY_MIN, chooseChatBackground, removeChatBackground, updateChatBackgroundPrefs, useChatBackgroundPrefs, useChatBackgroundUrl, type ChatBackgroundScope } from './chatBackgroundStore'
import { CHAT_BACKGROUND_EFFECTS, type ChatBackgroundEffect } from './chatBackgroundEffects'
import { SegmentedControl } from './SegmentedControl'
import { SettingsGroup, SettingsRow } from './SettingsParts'
import { AccentPicker } from './AccentPicker'
import { DEFAULT_HAZE, HAZE_LIMITS, resetHazeTuning, updateHazeTuning, useHazeTuning } from './hazeTuning'
import './chatBackground.css'

function Layer({ url, effect, opacity, empty, preview = false }: { url: string; effect: ChatBackgroundEffect; opacity: number; empty: boolean; preview?: boolean }) {
  const style = { '--chat-bg-image': `url("${url}")`, opacity } as CSSProperties
  return <div className={`chat-bg effect-${effect}${empty ? ' empty' : ''}${preview ? ' preview' : ''}`} style={style} aria-hidden="true">
    {effect === 'haze' && <><span className="chat-bg-soft" /><span className="chat-bg-strong" /></>}
  </div>
}

/** Behind the conversation, in Haze only, once an image is chosen. */
export function ChatBackgroundLayer({ empty }: { empty: boolean }) {
  const haze = useThemeStyle() === 'haze'
  const prefs = useChatBackgroundPrefs()
  const url = useChatBackgroundUrl(haze)
  if (!haze || !url || (prefs.scope === 'empty' && !empty)) return null
  return <Layer url={url} effect={prefs.effect} opacity={empty ? prefs.emptyOpacity : prefs.sessionOpacity} empty={empty} />
}

function Strength({ label, value, onChange }: { label: string; value: number; onChange: (value: number) => void }) {
  const percent = Math.round(value * 100)
  return <span className="chat-bg-strength">
    <input type="range" aria-label={label} min={Math.round(CHAT_BACKGROUND_OPACITY_MIN * 100)} max={Math.round(CHAT_BACKGROUND_OPACITY_MAX * 100)} value={percent} onChange={(event) => onChange(Number(event.target.value) / 100)} />
    <output>{percent}%</output>
  </span>
}

export function ChatBackgroundSettings() {
  const prefs = useChatBackgroundPrefs()
  const url = useChatBackgroundUrl(true)
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)
  const input = useRef<HTMLInputElement>(null)
  const pick = async (file: File | undefined) => {
    if (!file) return
    setBusy(true); setProblem(null)
    setProblem(await chooseChatBackground(file))
    setBusy(false)
  }
  const remove = async () => {
    setBusy(true); setProblem(null)
    try { await removeChatBackground() } catch { setProblem('The image could not be removed.') }
    setBusy(false)
  }
  return <SettingsGroup title="Chat background">
    <input ref={input} type="file" accept="image/*" hidden tabIndex={-1} aria-hidden="true" onChange={(event) => { const file = event.currentTarget.files?.[0]; event.currentTarget.value = ''; void pick(file) }} />
    <div className="settings-row stacked chat-bg-source">
      {prefs.hasImage
        ? <div className="chat-bg-preview">{url && <Layer url={url} effect={prefs.effect} opacity={prefs.emptyOpacity} empty preview />}<span>Empty chat at {Math.round(prefs.emptyOpacity * 100)}%</span></div>
        : <button type="button" className="chat-bg-choose" disabled={busy} onClick={() => input.current?.click()}><ImagePlus size={18} aria-hidden="true" />Choose an image</button>}
      {prefs.hasImage && <div className="chat-bg-actions">
        <button type="button" className="secondary-button small" disabled={busy} onClick={() => input.current?.click()}>Change</button>
        <button type="button" className="secondary-button small danger" disabled={busy} onClick={() => void remove()}>Remove</button>
      </div>}
      {problem && <p className="chat-bg-problem" role="alert">{problem}</p>}
    </div>
    {prefs.hasImage && <>
      <SettingsRow label="Effect"><SegmentedControl label="Background effect" value={prefs.effect} options={CHAT_BACKGROUND_EFFECTS} onChange={(effect) => updateChatBackgroundPrefs({ effect })} /></SettingsRow>
      <SettingsRow label="Show on"><SegmentedControl<ChatBackgroundScope> label="Show background on" value={prefs.scope} options={[{ value: 'empty', label: 'Empty only' }, { value: 'all', label: 'All conversations' }]} onChange={(scope) => updateChatBackgroundPrefs({ scope })} /></SettingsRow>
      <SettingsRow label="Empty chat visibility"><Strength label="Empty chat background visibility" value={prefs.emptyOpacity} onChange={(emptyOpacity) => updateChatBackgroundPrefs({ emptyOpacity })} /></SettingsRow>
      <SettingsRow label="Conversation visibility"><Strength label="Conversation background visibility" value={prefs.sessionOpacity} onChange={(sessionOpacity) => updateChatBackgroundPrefs({ sessionOpacity })} /></SettingsRow>
    </>}
  </SettingsGroup>
}

function Slider({ label, value, min, max, onChange, format }: { label: string; value: number; min: number; max: number; onChange: (value: number) => void; format: (value: number) => string }) {
  return <span className="chat-bg-strength">
    <input type="range" aria-label={label} min={Math.round(min * 100)} max={Math.round(max * 100)} value={Math.round(value * 100)} onChange={(event) => onChange(Number(event.target.value) / 100)} />
    <output>{format(value)}</output>
  </span>
}

/** Settings › Appearance › Haze: the backdrop's opacity, mist strength, panel shade and colour (owner, 9 November 2026). */
export function HazeTuningSettings() {
  const tuning = useHazeTuning()
  const changed = tuning.opacity !== DEFAULT_HAZE.opacity || tuning.strength !== DEFAULT_HAZE.strength || tuning.shade !== DEFAULT_HAZE.shade || tuning.tint !== DEFAULT_HAZE.tint
  const percent = (value: number) => `${Math.round(value * 100)}%`
  return <SettingsGroup title="Haze">
    <SettingsRow label="Background opacity" hint="Lower lets more of your desktop show through."><Slider label="Haze background opacity" value={tuning.opacity} min={HAZE_LIMITS.opacity[0]} max={HAZE_LIMITS.opacity[1]} format={percent} onChange={(opacity) => updateHazeTuning({ opacity })} /></SettingsRow>
    <SettingsRow label="Haze strength" hint="How strong the coloured mist is. 0% is a plain background."><Slider label="Haze strength" value={tuning.strength} min={HAZE_LIMITS.strength[0]} max={HAZE_LIMITS.strength[1]} format={percent} onChange={(strength) => updateHazeTuning({ strength })} /></SettingsRow>
    <SettingsRow label="Shade" hint="Calms the haze with the base colour."><Slider label="Haze shade" value={tuning.shade} min={HAZE_LIMITS.shade[0]} max={HAZE_LIMITS.shade[1]} format={percent} onChange={(shade) => updateHazeTuning({ shade })} /></SettingsRow>
    <SettingsRow label="Haze colour" hint="Neutral follows your accent colour."><AccentPicker value={tuning.tint} onChange={(tint) => updateHazeTuning({ tint })} /></SettingsRow>
    <SettingsRow label="Reset the haze"><button type="button" className="secondary-button small" disabled={!changed} onClick={resetHazeTuning}>Reset</button></SettingsRow>
  </SettingsGroup>
}
