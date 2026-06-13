import type { Metadata, Viewport } from 'next'
import './globals.css'

export const metadata: Metadata = {
  title: 'Bella — Open source AI cost visibility',
  description:
    'See what every model, provider, workspace, and team actually costs.',
  metadataBase: new URL('https://bellalabs.ai'),
  openGraph: {
    title: 'Bella — Open source AI cost visibility',
    description:
      'See what every model, provider, workspace, and team actually costs.',
    url: 'https://bellalabs.ai',
    siteName: 'Bella',
    type: 'website',
  },
}

export const viewport: Viewport = {
  themeColor: '#f5f2ed',
  colorScheme: 'light',
}

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode
}>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  )
}
