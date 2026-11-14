'use client'
// The employee's ID badge on its lanyard (React Bits Lanyard, three.js). Front: the badge. Back:
// what they do, their default personality and their skills. Picking another employee swings their
// badge in and turns it over; tapping the badge turns it back.
import { useEffect, useState } from 'react'
import dynamic from 'next/dynamic'
import { WhenVisible } from '@/components/motion'
import type { Person } from '@/content/roster'

const Lanyard = dynamic(() => import('@/components/bits/Lanyard'), { ssr: false })

export function Badge({ person, className = '' }: { person: Person; className?: string }) {
  const [side, setSide] = useState({ id: person.id, back: false, first: true })
  // A new employee: show their face first (state adjusted during render, as React recommends)...
  if (side.id !== person.id) setSide({ id: person.id, back: false, first: false })
  const back = side.id === person.id && side.back
  // ...then turn the badge over to their details.
  useEffect(() => {
    if (side.first || side.back) return
    const timer = window.setTimeout(() => setSide((current) => current.id === side.id ? { ...current, back: true } : current), 900)
    return () => window.clearTimeout(timer)
  }, [side.id, side.first, side.back])
  return <WhenVisible className={className}>
    <Lanyard frontImage={`/badges/${person.id}.png`} backImage={`/badges/${person.id}-back.png`} cardColor="#0b0b0c" strapColor={person.color} finish="glossy" strapLength={0.9} size={0.62} showBack={back} />
  </WhenVisible>
}
