'use client'

import { DogAsciiLogo } from '@/components/dog-ascii-logo'

export default function Home() {
  const appUrl =
       process.env.NEXT_PUBLIC_BELLA_APP_URL ?? 'https://app.bellalabs.ai'

  return (
    <main className="flex min-h-dvh items-center justify-center px-4 py-8">
      <section className="flex w-full flex-col items-center text-center">
        <DogAsciiLogo />
        <nav className="mt-5 flex items-center gap-5 text-sm">
          <a className="text-neutral underline underline-offset-4" href={appUrl} suppressHydrationWarning>
            Open Bella
          </a>
          <a
            className="text-neutral-600 underline underline-offset-4"
            href="https://github.com/seandowdall/bella"
          >
            GitHub
          </a>
        </nav>
      </section>
    </main>
  )
}
