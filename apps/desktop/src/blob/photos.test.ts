import { describe, expect, it } from 'vitest'
import { agentLook, normalizeLook, sameLook } from './look'
import { isProfilePhoto, PROFILE_PHOTOS, profilePhotoUrl } from './photos'

describe('profile photos', () => {
  it('lists the cropped pictures by id', () => {
    expect(PROFILE_PHOTOS.length).toBeGreaterThan(0)
    expect(PROFILE_PHOTOS.every((photo) => /^photo-\d\d$/.test(photo.id) && photo.url)).toBe(true)
  })

  it('keeps a known photo through a stored look and drops an unknown one', () => {
    const id = PROFILE_PHOTOS[0]!.id
    expect(normalizeLook({ shape: 'circle', photo: id })?.photo).toBe(id)
    expect(normalizeLook({ shape: 'circle', photo: 'photo-99x' })?.photo).toBeUndefined()
    expect(agentLook({ id: 'a', look: { shape: 'circle', photo: id } }).photo).toBe(id)
    expect(isProfilePhoto('char-thor')).toBe(false)
  })

  it('makes a photo part of what counts as a different look', () => {
    const id = PROFILE_PHOTOS[0]!.id
    expect(sameLook({ shape: 'circle' }, { shape: 'circle', photo: id })).toBe(false)
    expect(profilePhotoUrl({ photo: id })).toBe(PROFILE_PHOTOS[0]!.url)
    expect(profilePhotoUrl({})).toBeNull()
  })
})
