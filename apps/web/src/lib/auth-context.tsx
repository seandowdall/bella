"use client"

import { Spinner } from "@/components/ui/spinner"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { createContext, useContext, useEffect, useState } from "react"
import type { ReactNode } from "react"
import { useRouter } from "next/navigation"
import type { Organization, User } from "@/lib/dashboard-types"
import {
  getMe,
  getOrganizations,
  createOrganization as apiCreateOrganization,
  logout as apiLogout,
  getLoginUrl,
} from "@/lib/api"
import posthog from "posthog-js"

type AuthContextValue = {
  user: User
  organizations: Organization[]
  selectedOrganizationId: string
  selectedOrganization: Organization | undefined
  setSelectedOrganizationId: (id: string) => void
  loading: boolean
  error: string
  setError: (error: string) => void
  logout: () => Promise<void>
  createOrganization: (name: string) => Promise<Organization>
  login: () => void
}

const AuthContext = createContext<AuthContextValue | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const router = useRouter()
  const [user, setUser] = useState<User | null>(null)
  const [organizations, setOrganizations] = useState<Organization[]>([])
  const [selectedOrganizationId, setSelectedOrganizationId] = useState("")
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState("")

  useEffect(() => {
    const load = async () => {
      try {
        const authenticatedUser = await getMe()
        if (!authenticatedUser) {
          setUser(null)
          return
        }
        const orgs = await getOrganizations()
        setUser(authenticatedUser)
        setOrganizations(orgs)
        setSelectedOrganizationId(orgs[0]?.id ?? "")
        posthog.identify(authenticatedUser.github_login, {
          name: authenticatedUser.name ?? authenticatedUser.github_login,
          github_login: authenticatedUser.github_login,
        })
      } catch (e) {
        setError(
          e instanceof Error ? e.message : "Could not load the dashboard.",
        )
      } finally {
        setLoading(false)
      }
    }
    void load()
  }, [])

  const selectedOrganization =
    organizations.find((o) => o.id === selectedOrganizationId) ??
    organizations[0]

  const handleLogout = async () => {
    posthog.capture("logout_clicked")
    await apiLogout()
    posthog.reset()
    setUser(null)
    setOrganizations([])
  }

  const handleCreateOrganization = async (name: string) => {
    const org = await apiCreateOrganization(name)
    setOrganizations((current) => [...current, org])
    setSelectedOrganizationId(org.id)
    return org
  }

  const login = () => {
    window.location.assign(getLoginUrl())
  }

  useEffect(() => {
    if (!loading && !user) {
      if (error) return
      router.replace("/login")
    }
  }, [error, loading, router, user])

  if (!user) {
    if (loading) {
      return (
        <main className="grid min-h-svh place-items-center">
          <div className="text-muted-foreground flex items-center gap-2 text-sm">
            <Spinner />
            Loading Bella
          </div>
        </main>
      )
    }
    if (error) {
      return (
        <main className="grid min-h-svh place-items-center p-6">
          <div className="flex w-full max-w-md flex-col gap-4">
            <Alert variant="destructive">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
            <Button onClick={() => window.location.reload()}>Retry</Button>
          </div>
        </main>
      )
    }
    return null
  }

  return (
    <AuthContext.Provider
      value={{
        user,
        organizations,
        selectedOrganizationId,
        selectedOrganization,
        setSelectedOrganizationId,
        loading,
        error,
        setError,
        logout: handleLogout,
        createOrganization: handleCreateOrganization,
        login,
      }}
    >
      {children}
    </AuthContext.Provider>
  )
}

export function useAuth() {
  const ctx = useContext(AuthContext)
  if (!ctx) throw new Error("useAuth must be used within AuthProvider")
  return ctx
}
