import { useEffect, useRef, useState, type ChangeEvent } from 'react'
import { ChevronLeft, ChevronRight, ImageUp, X } from 'lucide-react'
import { usePhotoOwners } from './photoOwners'
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
export function LookEditor({ look, color, colorHex, name, locked = false, runtimeId, agentId = null, onLookChange, onColorChange }: {
  look: BlobLook
  /** This employee, so its own photo never shows as taken. */
  agentId?: string | null
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
  // Pages keep the picker small as portraits are added (owner, 9 November 2026): the first page
  // starts with Upload, your own photo and No photo, then the portraits.
  const owners = usePhotoOwners()
  const lead = 2 + (uploaded ? 1 : 0)
  const all = PROFILE_PHOTOS.map((photo, index) => ({ photo, index }))
  const pageOf = (index: number) => index + lead < PHOTOS_PER_PAGE ? 0 : 1 + Math.floor((index + lead - PHOTOS_PER_PAGE) / PHOTOS_PER_PAGE)
  const pages = all.length ? pageOf(all.length - 1) + 1 : 1
  const currentIndex = all.findIndex(({ photo }) => photo.id === current)
  const [page, setPage] = useState(() => currentIndex >= 0 ? pageOf(currentIndex) : 0)
  useEffect(() => { if (page > pages - 1) setPage(pages - 1) }, [page, pages])
  const shown = all.filter(({ index }) => pageOf(index) === page)
  return <div className="look-editor photo-look-editor" style={{ ['--look-accent' as string]: colorHex }}>
    <div className="look-stage">
      <div className="look-stage-hero photo-look-hero"><BlobCanvas color={colorHex} size={112} look={look} label={`${name} preview`} /></div>
      <div className="photo-look-row" aria-hidden="true"><BlobCanvas decorative color={colorHex} size={32} look={look} label={name} /><span>{name}</span></div>
    </div>
    <div className="look-panel">
      <span className="look-section-label">Profile photo</span>
      <div className="profile-photo-grid" role="radiogroup" aria-label="Profile photo">
        {page === 0 && <>
          <button type="button" className="profile-photo-option profile-photo-upload" disabled={locked} title="Upload your own photo" aria-label="Upload your own photo" onClick={() => upload.current?.click()}>
            <ImageUp size={18} aria-hidden="true" />
          </button>
          {uploaded && <PhotoOption label="Your photo" selected locked={locked} onPick={() => undefined}>
            <img src={uploaded} alt="" draggable={false} />
          </PhotoOption>}
          <PhotoOption label="No photo" selected={!current} locked={locked} onPick={() => choose(null)}>
            <BlobCanvas decorative color={colorHex} size={40} look={{ shape: 'circle' }} label="No photo" />
          </PhotoOption>
        </>}
        {shown.map(({ photo, index }) => { const owner = owners.get(photo.id); const taken = !!owner && owner.agentId !== agentId; return <PhotoOption key={photo.id} label={taken ? `Profile photo ${index + 1}, worn by ${owner!.name}` : `Profile photo ${index + 1}`} selected={current === photo.id} locked={locked || taken} taken={taken ? owner!.name : undefined} onPick={() => choose(photo.id)}>
          <img src={photo.url} alt="" draggable={false} />
        </PhotoOption> })}
        <input ref={upload} type="file" accept="image/png,image/jpeg,image/webp,image/gif" hidden onChange={onFile} />
      </div>
      {pages > 1 && <div className="profile-photo-pager" role="group" aria-label="Photo pages">
        <button type="button" className="icon-button tiny" aria-label="Previous photos" disabled={page === 0} onClick={() => setPage(page - 1)}><ChevronLeft size={14} aria-hidden="true" /></button>
        <span className="profile-photo-dots">{Array.from({ length: pages }, (_, index) => <button key={index} type="button" aria-label={`Photos page ${index + 1}`} aria-current={index === page ? 'page' : undefined} className={index === page ? 'on' : undefined} onClick={() => setPage(index)} />)}</span>
        <button type="button" className="icon-button tiny" aria-label="More photos" disabled={page >= pages - 1} onClick={() => setPage(page + 1)}><ChevronRight size={14} aria-hidden="true" /></button>
      </div>}
      {uploadError && <p className="profile-photo-error" role="alert"><X size={12} aria-hidden="true" />{uploadError}</p>}
      <p className="profile-photo-hint">A photo you upload is cropped to a square and kept small, on this computer.</p>
      {!current && <div className="look-colour"><span className="look-section-label">Colour</span><SwatchGrid value={color} onChange={onColorChange} disabled={locked} /></div>}
    </div>
  </div>
}

const PHOTOS_PER_PAGE = 24

function PhotoOption({ label, selected, locked, taken, onPick, children }: { label: string; selected: boolean; locked: boolean; taken?: string; onPick: () => void; children: React.ReactNode }) {
  return <button type="button" role="radio" aria-checked={selected} aria-label={label} title={taken ? `Worn by ${taken}. Change ${taken}'s photo first.` : label} tabIndex={selected ? 0 : -1} disabled={locked} className={`profile-photo-option${selected ? ' selected' : ''}${taken ? ' taken' : ''}`} onClick={onPick}>
    {children}
  </button>
}
