import type * as React from 'react'
import {
  ArrowLeftIcon,
  BotIcon,
  BoxesIcon,
  Building2Icon,
  LayoutDashboardIcon,
  PlugIcon,
  SettingsIcon,
  UserIcon,
} from 'lucide-react'
import Link from 'next/link'
import { usePathname } from 'next/navigation'
import { NavMain } from '@/components/nav-main'
import { NavUser } from '@/components/nav-user'
import { OrganizationSwitcher } from '@/components/organization-switcher'
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@/components/ui/sidebar'
import type { Organization, User } from '@/lib/dashboard-types'

const navigation = [
  { title: 'Home', icon: BotIcon, href: '/' },
  { title: 'Data', icon: LayoutDashboardIcon, href: '/data' },
  { title: 'Providers', icon: BoxesIcon, href: '/providers' },
]

const settingsNavigation = [
  { title: 'Profile', icon: UserIcon, href: '/settings' },
  { title: 'Organization', icon: Building2Icon, href: '/settings/organization' },
  { title: 'AI', icon: BotIcon, href: '/settings/ai' },
  { title: 'Integrations', icon: PlugIcon, href: '/settings/integrations' },
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
  const pathname = usePathname()
  const isSettings = pathname.startsWith('/settings')

  return (
    <Sidebar collapsible="offcanvas" {...props}>
      {isSettings ? (
        <SidebarHeader>
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton asChild>
                <Link href="/">
                  <ArrowLeftIcon />
                  <span>Back to app</span>
                </Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarHeader>
      ) : (
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
      )}
      <SidebarContent>
        {isSettings ? (
          <SidebarGroup>
            <SidebarGroupLabel>Settings</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {settingsNavigation.map((item) => {
                  const Icon = item.icon
                  return (
                    <SidebarMenuItem key={item.title}>
                      <SidebarMenuButton
                        asChild
                        tooltip={item.title}
                        isActive={pathname === item.href}
                      >
                        <Link href={item.href}>
                          <Icon />
                          <span>{item.title}</span>
                        </Link>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  )
                })}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        ) : (
          <>
            <NavMain items={navigation} />
            <SidebarGroup className="mt-auto">
              <SidebarGroupContent>
                <SidebarMenu>
                  <SidebarMenuItem>
                    <SidebarMenuButton asChild tooltip="Settings">
                      <Link href="/settings">
                        <SettingsIcon />
                        <span>Settings</span>
                      </Link>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          </>
        )}
      </SidebarContent>
      <SidebarFooter>
        <NavUser user={user} onLogout={onLogout} />
      </SidebarFooter>
    </Sidebar>
  )
}
