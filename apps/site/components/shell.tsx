import type { ReactNode } from 'react'
import { SmoothScroll } from '@/components/motion'
import { EmployeeDialog } from '@/components/employees'
import { Nav } from '@/components/sections/Nav'
import { Footer } from '@/components/sections/Footer'

/** Every page but the home page: the nav, the content, the footer, and the employee dialog. */
export function PageShell({ children }: { children: ReactNode }) {
  return <>
    <SmoothScroll />
    <Nav />
    <main className="pt-16">{children}</main>
    <Footer />
    <EmployeeDialog />
  </>
}
