'use client'

import { PlusIcon } from 'lucide-react'
import { usePathname } from 'next/navigation'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/separator'
import { SidebarTrigger } from '@/components/ui/sidebar'
import { useProviderDialog } from '@/components/provider-dialog-context'
import type { Organization } from '@/lib/dashboard-types'

export function SiteHeader({
  organization,
  onCreateOrganization,
}: {
  organization?: Organization
  onCreateOrganization: () => void
}) {
  const pathname = usePathname()
  const { setOpen: setProviderOpen } = useProviderDialog()
  const isProvidersPage = pathname === '/providers'
  const canManageProviders =
    organization?.role === 'owner' || organization?.role === 'admin'

  return (
    <header className="flex h-(--header-height) shrink-0 items-center border-b">
      <div className="flex w-full items-center gap-2 px-4 lg:px-6">
        <SidebarTrigger className="-ml-1" />
        <Separator orientation="vertical" className="mx-1 h-4" />
        <div className="min-w-0 flex-1">
          <h1 className="truncate text-sm font-medium">
            {organization?.name ?? 'Overview'}
          </h1>
          {organization && (
            <p className="text-muted-foreground hidden truncate text-xs sm:block">
              {organization.default_workspace.name} workspace
            </p>
          )}
        </div>
        <Button
          size="sm"
          disabled={isProvidersPage && !canManageProviders}
          onClick={
            isProvidersPage
              ? () => setProviderOpen(true)
              : onCreateOrganization
          }
        >
          <PlusIcon data-icon="inline-start" />
          <span className="hidden sm:inline">
            {isProvidersPage ? 'Add provider' : 'New organization'}
          </span>
          <span className="sm:hidden">
            {isProvidersPage ? 'Add' : 'New'}
          </span>
        </Button>
      </div>
    </header>
  )
}
