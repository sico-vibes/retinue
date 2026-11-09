import { useRef, useState, type ChangeEvent } from 'react'
import { ImageUp, X } from 'lucide-react'
import { BlobCanvas } from '../blob/BlobCanvas'
import { PROFILE_PHOTOS, isUploadedPhoto, photoFromFile } from '../blob/photos'
import type { BlobLook } from '../blob/look'
import { rememberLookRuntime } from '../blob/haze/runtimeRegistry'
import { SwatchGrid } from './SwatchGrid'
import '../blob/photos.css'

/**
 * An employee's look (owner, 9 November 2026): a round profile picture, as in
 * Slack or Teams. Choose one of the portraits, upload your own, or leave the
 * blank silhouette. Every change is a draft until the page saves.
 */
export function LookEditor({ look, color, colorHex, name, locked = false, runtimeId, onLookChange, onColorChange }: {
  look: BlobLook
  /** The employee's coding agent, for the badge on the picture. */
  runtimeId?: string | null
  color: string
  colorHex: string
  name: string
  /** Every control here is inert while the employee is locked. */
  locked?: boolean
  /** An optional colour is saved with the look in the same draft update. */
  onLookChange: (look: BlobLook, color?: string) => void
  onColorChange: (color: string) => void
}) {
  rememberLookRuntime(look, runtimeId)
  const upload = useRef<HTMLInputElement>(null)
  const [uploadError, setUploadError] = useState<string | null>(null)
  const current = look.photo ?? null
  const choose = (photo: BlobLook['photo'] | null) => {
    const { photo: _photo, ...rest } = look
    onLookChange(photo ? { ...rest, photo } : rest)
  }
  const onFile = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    event.target.value = ''
    if (!file) return
    setUploadError(null)
    try { choose(await photoFromFile(file)) } catch (error) { setUploadError(error instanceof Error ? error.message : 'That picture could not be read.') }
  }
  const uploaded = isUploadedPhoto(current) ? current : null
  return <div className="look-editor photo-look-editor" style={{ ['--look-accent' as string]: colorHex }}>
    <div className="look-stage">
      <div className="look-stage-hero photo-look-hero"><BlobCanvas color={colorHex} size={112} look={look} label={`${name} preview`} /></div>
      <div className="photo-look-row" aria-hidden="true"><BlobCanvas decorative color={colorHex} size={32} look={look} label={name} /><span>{name}</span></div>
    </div>
    <div className="look-panel">
      <span className="look-section-label">Profile photo</span>
      <div className="profile-photo-grid" role="radiogroup" aria-label="Profile photo">
        <PhotoOption label="No photo" selected={!current} locked={locked} onPick={() => choose(null)}>
          <BlobCanvas decorative color={colorHex} size={40} look={{ shape: 'circle' }} label="No photo" />
        </PhotoOption>
        {PROFILE_PHOTOS.map((photo, index) => <PhotoOption key={photo.id} label={`Profile photo ${index + 1}`} selected={current === photo.id} locked={locked} onPick={() => choose(photo.id)}>
          <img src={photo.url} alt="" draggable={false} />
        </PhotoOption>)}
        {uploaded && <PhotoOption label="Your photo" selected locked={locked} onPick={() => undefined}>
          <img src={uploaded} alt="" draggable={false} />
        </PhotoOption>}
        <button type="button" className="profile-photo-option profile-photo-upload" disabled={locked} title="Upload a photo" aria-label="Upload a photo" onClick={() => upload.current?.click()}>
          <ImageUp size={18} aria-hidden="true" />
        </button>
        <input ref={upload} type="file" accept="image/png,image/jpeg,image/webp,image/gif" hidden onChange={onFile} />
      </div>
      {uploadError && <p className="profile-photo-error" role="alert"><X size={12} aria-hidden="true" />{uploadError}</p>}
      <p className="profile-photo-hint">A photo you upload is cropped to a square and kept small, on this computer.</p>
      {!current && <div className="look-colour"><span className="look-section-label">Colour</span><SwatchGrid value={color} onChange={onColorChange} disabled={locked} /></div>}
    </div>
  </div>
}

function PhotoOption({ label, selected, locked, onPick, children }: { label: string; selected: boolean; locked: boolean; onPick: () => void; children: React.ReactNode }) {
  return <button type="button" role="radio" aria-checked={selected} aria-label={label} title={label} tabIndex={selected ? 0 : -1} disabled={locked} className={`profile-photo-option${selected ? ' selected' : ''}`} onClick={onPick}>
    {children}
  </button>
}
