"use client"

import {
  Building2Icon,
  ChevronsUpDownIcon,
  PlusIcon,
} from "lucide-react"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/sidebar"
import type { Organization } from "@/lib/dashboard-types"

type OrganizationSwitcherProps = {
  organizations: Organization[]
  selectedOrganizationId: string
  onOrganizationChange: (organizationId: string) => void
  onCreateOrganization: () => void
}

export function OrganizationSwitcher({
  organizations,
  selectedOrganizationId,
  onOrganizationChange,
  onCreateOrganization,
}: OrganizationSwitcherProps) {
  const { isMobile } = useSidebar()
  const activeOrganization = organizations.find(
    (organization) => organization.id === selectedOrganizationId
  )

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <SidebarMenuButton
              size="lg"
              className="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
            >
              <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground">
                <Building2Icon />
              </div>
              <div className="grid flex-1 text-left text-sm leading-tight">
                <span className="truncate font-medium">
                  {activeOrganization?.name ?? "Select organization"}
                </span>
                <span className="truncate text-xs text-muted-foreground capitalize">
                  {activeOrganization?.role ?? "Organization"}
                </span>
              </div>
              <ChevronsUpDownIcon className="ml-auto" />
            </SidebarMenuButton>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            align="start"
            side={isMobile ? "bottom" : "right"}
            sideOffset={4}
            className="min-w-56 rounded-lg"
          >
            <DropdownMenuLabel>Organizations</DropdownMenuLabel>
            <DropdownMenuGroup>
              {organizations.map((organization) => (
                <DropdownMenuItem
                  key={organization.id}
                  onClick={() => onOrganizationChange(organization.id)}
                >
                  <div className="flex size-6 items-center justify-center rounded-md border">
                    <Building2Icon />
                  </div>
                  <span className="truncate">{organization.name}</span>
                </DropdownMenuItem>
              ))}
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            <DropdownMenuGroup>
              <DropdownMenuItem onClick={onCreateOrganization}>
                <div className="flex size-6 items-center justify-center rounded-md border bg-transparent">
                  <PlusIcon />
                </div>
                <span className="font-medium text-muted-foreground">
                  New organization
                </span>
              </DropdownMenuItem>
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  )
}
