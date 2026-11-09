'use client'

import dynamic from 'next/dynamic'
import { DeferUntilNear } from '@/components/ui/DeferUntilNear'

// Below-the-fold sections with JavaScript. Each is its own chunk, fetched only when it nears the
// viewport. A new interactive section gets one line here and one line in app/page.tsx.
const CompanyStorySection = dynamic(() => import('./CompanyStory').then((module) => module.CompanyStory))
const HiringSimulatorSection = dynamic(() => import('./HiringSimulator').then((module) => module.HiringSimulator))
const IdeaToShippedSection = dynamic(() => import('./IdeaToShipped').then((module) => module.IdeaToShipped))
const WorkingDaySection = dynamic(() => import('./WorkingDay').then((module) => module.WorkingDay))
const ControlSection = dynamic(() => import('./Control').then((module) => module.Control))
const WithinReachSection = dynamic(() => import('./WithinReach').then((module) => module.WithinReach))

export const CompanyStory = () => <DeferUntilNear><CompanyStorySection /></DeferUntilNear>
export const HiringSimulator = () => <DeferUntilNear><HiringSimulatorSection /></DeferUntilNear>
export const IdeaToShipped = () => <DeferUntilNear><IdeaToShippedSection /></DeferUntilNear>
export const WorkingDay = () => <DeferUntilNear><WorkingDaySection /></DeferUntilNear>
export const Control = () => <DeferUntilNear><ControlSection /></DeferUntilNear>
export const WithinReach = () => <DeferUntilNear><WithinReachSection /></DeferUntilNear>
