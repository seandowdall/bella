"use client"

import type { CSSProperties, FormEvent } from "react"
import { useState } from "react"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar"
import { Spinner } from "@/components/ui/spinner"
import { AppSidebar } from "@/components/app-sidebar"
import { SiteHeader } from "@/components/site-header"
import { ProviderDialogProvider } from "@/components/provider-dialog-context"
import { AuthProvider, useAuth } from "@/lib/auth-context"

function DashboardContent({ children }: { children: React.ReactNode }) {
  const {
    user,
    organizations,
    selectedOrganization,
    setSelectedOrganizationId,
    logout,
    createOrganization,
  } = useAuth()
  const [organizationName, setOrganizationName] = useState("")
  const [createOpen, setCreateOpen] = useState(false)
  const [creating, setCreating] = useState(false)
  const [createError, setCreateError] = useState("")

  const handleCreateOrganization = async (
    event: FormEvent<HTMLFormElement>,
  ) => {
    event.preventDefault()
    setCreating(true)
    setCreateError("")
    try {
      await createOrganization(organizationName)
      setOrganizationName("")
      setCreateOpen(false)
    } catch (e) {
      setCreateError(
        e instanceof Error ? e.message : "Could not create the organization.",
      )
    } finally {
      setCreating(false)
    }
  }

  return (
    <ProviderDialogProvider>
      <SidebarProvider
        style={
          {
            "--sidebar-width": "16rem",
            "--header-height": "3rem",
          } as CSSProperties
        }
      >
        <AppSidebar
          user={user}
          organizations={organizations}
          selectedOrganizationId={selectedOrganization?.id ?? ""}
          onOrganizationChange={setSelectedOrganizationId}
          onCreateOrganization={() => {
            setCreateError("")
            setCreateOpen(true)
          }}
          onLogout={() => void logout()}
          variant="inset"
        />
        <SidebarInset>
          <SiteHeader
            organization={selectedOrganization}
          />
          <div className="@container/main flex flex-1 flex-col gap-4 p-4 lg:p-6">
            {children}
          </div>
        </SidebarInset>
      </SidebarProvider>

      <Sheet open={createOpen} onOpenChange={setCreateOpen}>
        <SheetContent>
          <form
            className="flex min-h-0 flex-1 flex-col"
            onSubmit={handleCreateOrganization}
          >
            <SheetHeader>
              <SheetTitle>Create organization</SheetTitle>
              <SheetDescription>
                Organizations isolate provider credentials, usage, costs, and
                team access.
              </SheetDescription>
            </SheetHeader>
            <FieldGroup className="p-4">
              <Field>
                <FieldLabel htmlFor="organization-name">
                  Organization name
                </FieldLabel>
                <Input
                  id="organization-name"
                  value={organizationName}
                  maxLength={80}
                  placeholder="Acme AI"
                  onChange={(event) =>
                    setOrganizationName(event.target.value)
                  }
                  required
                  autoFocus
                />
                <FieldDescription>
                  A default workspace is created automatically.
                </FieldDescription>
              </Field>
              {createError && (
                <Alert variant="destructive">
                  <AlertDescription>{createError}</AlertDescription>
                </Alert>
              )}
            </FieldGroup>
            <SheetFooter>
              <Button type="submit" disabled={creating}>
                {creating && <Spinner data-icon="inline-start" />}
                {creating ? "Creating" : "Create organization"}
              </Button>
            </SheetFooter>
          </form>
        </SheetContent>
      </Sheet>
    </ProviderDialogProvider>
  )
}

export default function DashboardLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <AuthProvider>
      <DashboardContent>{children}</DashboardContent>
    </AuthProvider>
  )
}
