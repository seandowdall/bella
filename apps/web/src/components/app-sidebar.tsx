import type * as React from 'react'
import {
  BoxesIcon,
  BotIcon,
  LayoutDashboardIcon,
} from 'lucide-react'
import { NavMain } from '@/components/nav-main'
import { NavUser } from '@/components/nav-user'
import { OrganizationSwitcher } from '@/components/organization-switcher'
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
} from '@/components/ui/sidebar'
import type { Organization, User } from '@/lib/dashboard-types'

const navigation = [
  { title: 'Home', icon: BotIcon, href: '/' },
  { title: 'Data', icon: LayoutDashboardIcon, href: '/data' },
  { title: 'Providers', icon: BoxesIcon, href: '/providers' },
]

type AppSidebarProps = React.ComponentProps<typeof Sidebar> & {
  user: User
  organizations: Organization[]
  selectedOrganizationId: string
  onOrganizationChange: (organizationId: string) => void
  onCreateOrganization: () => void
  onLogout: () => void
}

export function AppSidebar({
  user,
  organizations,
  selectedOrganizationId,
  onOrganizationChange,
  onCreateOrganization,
  onLogout,
  ...props
}: AppSidebarProps) {
  return (
    <Sidebar collapsible="offcanvas" {...props}>
      <SidebarHeader className="gap-3">
        <div className="px-2 pt-1">
          <p className="text-primary text-xs font-bold tracking-[0.14em] uppercase">
            Bella
          </p>
          <p className="text-sidebar-foreground mt-1 text-sm font-semibold">
            AI cost visibility
          </p>
        </div>
        <OrganizationSwitcher
          organizations={organizations}
          selectedOrganizationId={selectedOrganizationId}
          onOrganizationChange={onOrganizationChange}
          onCreateOrganization={onCreateOrganization}
        />
      </SidebarHeader>
      <SidebarContent>
        <NavMain items={navigation} />
      </SidebarContent>
      <SidebarFooter>
        <NavUser user={user} onLogout={onLogout} />
      </SidebarFooter>
    </Sidebar>
  )
}
