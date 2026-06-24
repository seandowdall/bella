"use client"

import { FormEvent, useEffect, useMemo, useState } from "react"
import {
  CableIcon,
  ChevronsUpDownIcon,
  ExternalLinkIcon,
  MoreHorizontalIcon,
  PencilIcon,
  RefreshCwIcon,
  Trash2Icon,
} from "lucide-react"
import { Alert, AlertDescription } from "@/components/ui/alert"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import { Spinner } from "@/components/ui/spinner"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { ProviderIcon } from "@/components/provider-icon"
import { useProviderDialog } from "@/components/provider-dialog-context"
import posthog from "posthog-js"
import {
  connectProviderAccount,
  deleteProviderAccount,
  getProviderAccounts,
  getProviderCatalog,
  syncProviderAccount,
  updateProviderAccount,
} from "@/lib/api"
import type {
  Organization,
  ProviderAccount,
  ProviderDefinition,
} from "@/lib/dashboard-types"

const visibleProviderIds = new Set(["anthropic", "openai"])

const statusLabels = {
  saved_unverified: "Saved, unverified",
  verified: "Verified",
  invalid_credentials: "Invalid credentials",
  insufficient_permissions: "Missing permissions",
  validation_unavailable: "Validation unavailable",
  disabled: "Disabled",
}

export function ProviderAccounts({
  organization,
}: {
  organization?: Organization
}) {
  const { open, setOpen } = useProviderDialog()
  const [catalog, setCatalog] = useState<ProviderDefinition[]>([])
  const [accounts, setAccounts] = useState<ProviderAccount[]>([])
  const [selectedProviderId, setSelectedProviderId] = useState("")
  const [comboboxOpen, setComboboxOpen] = useState(false)
  const [displayName, setDisplayName] = useState("")
  const [secret, setSecret] = useState("")
  const [editingAccount, setEditingAccount] =
    useState<ProviderAccount | null>(null)
  const [accountToDelete, setAccountToDelete] =
    useState<ProviderAccount | null>(null)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [syncingAccountId, setSyncingAccountId] = useState("")
  const [error, setError] = useState("")

  const canManage =
    organization?.role === "owner" || organization?.role === "admin"
  const selectedProvider = useMemo(
    () => catalog.find((provider) => provider.id === selectedProviderId),
    [catalog, selectedProviderId],
  )

  const handleOpenChange = (nextOpen: boolean) => {
    setOpen(nextOpen)
    if (!nextOpen) {
      setComboboxOpen(false)
      setDisplayName("")
      setSecret("")
      setEditingAccount(null)
      setError("")
    }
  }

  const openEdit = (account: ProviderAccount) => {
    setEditingAccount(account)
    setSelectedProviderId(account.provider)
    setDisplayName(account.display_name)
    setSecret("")
    setError("")
    setOpen(true)
  }

  useEffect(() => {
    const load = async () => {
      if (!organization) return
      setLoading(true)
      setError("")
      try {
        const [definitions, providerAccounts] = await Promise.all([
          getProviderCatalog(),
          getProviderAccounts(organization.id),
        ])
        const visibleDefinitions = definitions.filter((provider) =>
          visibleProviderIds.has(provider.id),
        )
        setCatalog(visibleDefinitions)
        setAccounts(
          providerAccounts.filter((account) =>
            visibleProviderIds.has(account.provider),
          ),
        )
        setSelectedProviderId(
          (current) => current || visibleDefinitions[0]?.id || "",
        )
      } catch (e) {
        setError(e instanceof Error ? e.message : "Could not load providers.")
      } finally {
        setLoading(false)
      }
    }
    void load()
  }, [organization])

  const handleConnect = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!organization || !selectedProvider) return
    setSaving(true)
    setError("")
    try {
      const account = editingAccount
        ? await updateProviderAccount(
            organization.id,
            editingAccount.id,
            displayName,
          )
        : await connectProviderAccount({
            organizationId: organization.id,
            workspaceId: organization.default_workspace.id,
            provider: selectedProvider.id,
            displayName,
            secret,
          })
      if (!editingAccount) {
        posthog.capture("provider_connected", {
          provider: selectedProvider.id,
          provider_name: selectedProvider.name,
          display_name: displayName,
        })
      }
      setAccounts((current) => [
        ...current.filter((item) => item.id !== account.id),
        account,
      ])
      handleOpenChange(false)
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not connect provider.")
    } finally {
      setSaving(false)
    }
  }

  const handleDelete = async () => {
    if (!organization || !accountToDelete) return
    setDeleting(true)
    setError("")
    try {
      await deleteProviderAccount(organization.id, accountToDelete.id)
      setAccounts((current) =>
        current.filter((account) => account.id !== accountToDelete.id),
      )
      setAccountToDelete(null)
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not disconnect provider.")
    } finally {
      setDeleting(false)
    }
  }

  const handleSync = async (account: ProviderAccount) => {
    if (!organization) return
    setSyncingAccountId(account.id)
    setError("")
    try {
      await syncProviderAccount(organization.id, account.id)
      const providerAccounts = await getProviderAccounts(organization.id)
      setAccounts(
        providerAccounts.filter((item) => visibleProviderIds.has(item.provider)),
      )
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not sync provider account.")
    } finally {
      setSyncingAccountId("")
    }
  }

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>Provider accounts</CardTitle>
          <CardDescription>
            Credentials and usage sources for{" "}
            {organization?.name ?? "this organization"}.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {error && !open && (
            <Alert variant="destructive" className="mb-4">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          <div className="overflow-hidden rounded-lg border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Provider</TableHead>
                  <TableHead>Account</TableHead>
                  <TableHead>Workspace</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead className="w-12">
                    <span className="sr-only">Actions</span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {loading ? (
                  <TableRow>
                    <TableCell colSpan={5} className="h-32 text-center">
                      <Spinner className="mx-auto" />
                    </TableCell>
                  </TableRow>
                ) : accounts.length ? (
                  accounts.map((account) => {
                    const definition = catalog.find(
                      (provider) => provider.id === account.provider,
                    )
                    return (
                      <TableRow key={account.id}>
                        <TableCell className="font-medium">
                          <div className="flex items-center gap-2">
                            <ProviderIcon
                              provider={account.provider}
                              name={definition?.name ?? account.provider}
                            />
                            <span>{definition?.name ?? account.provider}</span>
                          </div>
                        </TableCell>
                        <TableCell>
                          <div>{account.display_name}</div>
                          <div className="text-xs text-muted-foreground">
                            Key fingerprint {account.credential_fingerprint}
                          </div>
                        </TableCell>
                        <TableCell>{account.workspace_name}</TableCell>
                        <TableCell>
                          <Badge
                            variant={
                              account.status === "invalid_credentials" ||
                              account.status === "insufficient_permissions"
                                ? "destructive"
                                : "outline"
                            }
                          >
                            {statusLabels[account.status]}
                          </Badge>
                          {account.validation_error && (
                            <p className="mt-1 max-w-64 text-xs text-muted-foreground">
                              {account.validation_error}
                            </p>
                          )}
                          {account.last_sync_error && (
                            <p className="mt-1 max-w-64 text-xs text-destructive">
                              {account.last_sync_error}
                            </p>
                          )}
                          <p className="mt-1 text-xs text-muted-foreground">
                            Last sync {formatDateTime(account.last_synced_at)}
                          </p>
                        </TableCell>
                        <TableCell>
                          <DropdownMenu>
                            <DropdownMenuTrigger asChild>
                              <Button
                                size="icon-sm"
                                variant="ghost"
                                disabled={!canManage}
                              >
                                <MoreHorizontalIcon />
                                <span className="sr-only">
                                  Open options for {account.display_name}
                                </span>
                              </Button>
                            </DropdownMenuTrigger>
                            <DropdownMenuContent align="end">
                              <DropdownMenuGroup>
                                <DropdownMenuItem
                                  disabled={
                                    account.provider !== "openai" ||
                                    account.status !== "verified" ||
                                    syncingAccountId === account.id
                                  }
                                  onSelect={(event) => {
                                    event.preventDefault()
                                    void handleSync(account)
                                  }}
                                >
                                  {syncingAccountId === account.id ? (
                                    <Spinner />
                                  ) : (
                                    <RefreshCwIcon />
                                  )}
                                  Sync now
                                </DropdownMenuItem>
                                <DropdownMenuItem
                                  onSelect={() => openEdit(account)}
                                >
                                  <PencilIcon />
                                  Edit account
                                </DropdownMenuItem>
                              </DropdownMenuGroup>
                              <DropdownMenuSeparator />
                              <DropdownMenuGroup>
                                <DropdownMenuItem
                                  variant="destructive"
                                  onSelect={() => setAccountToDelete(account)}
                                >
                                  <Trash2Icon />
                                  Delete account
                                </DropdownMenuItem>
                              </DropdownMenuGroup>
                            </DropdownMenuContent>
                          </DropdownMenu>
                        </TableCell>
                      </TableRow>
                    )
                  })
                ) : (
                  <TableRow>
                    <TableCell colSpan={5} className="p-0">
                      <Empty className="min-h-56 border-0">
                        <EmptyHeader>
                          <EmptyMedia variant="icon">
                            <CableIcon />
                          </EmptyMedia>
                          <EmptyTitle>No provider accounts</EmptyTitle>
                          <EmptyDescription>
                            Connect an AI provider to begin importing usage and
                            calculating costs.
                          </EmptyDescription>
                        </EmptyHeader>
                      </Empty>
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
          </div>
        </CardContent>
      </Card>

      <Sheet open={open} onOpenChange={handleOpenChange}>
        <SheetContent>
          <form className="flex min-h-0 flex-1 flex-col" onSubmit={handleConnect}>
            <SheetHeader>
              <SheetTitle>
                {editingAccount ? "Edit provider account" : "Connect provider"}
              </SheetTitle>
              <SheetDescription>
                {editingAccount
                  ? "Update account metadata. The stored admin credential remains hidden and unchanged."
                  : "Store an organization credential for usage and cost ingestion."}
              </SheetDescription>
            </SheetHeader>
            <FieldGroup className="overflow-y-auto p-4">
              <Field>
                <FieldLabel>Provider</FieldLabel>
                {editingAccount ? (
                  <div className="flex h-8 items-center gap-2 rounded-lg border px-2.5 text-sm">
                    {selectedProvider && (
                      <ProviderIcon
                        provider={selectedProvider.id}
                        name={selectedProvider.name}
                        compact
                      />
                    )}
                    <span>{selectedProvider?.name}</span>
                  </div>
                ) : (
                  <Popover open={comboboxOpen} onOpenChange={setComboboxOpen}>
                    <PopoverTrigger asChild>
                      <Button
                        variant="outline"
                        role="combobox"
                        aria-expanded={comboboxOpen}
                        className="w-full justify-between"
                      >
                        {selectedProvider ? (
                          <span className="flex items-center gap-2">
                            <ProviderIcon
                              provider={selectedProvider.id}
                              name={selectedProvider.name}
                              compact
                            />
                            {selectedProvider.name}
                          </span>
                        ) : (
                          "Search providers"
                        )}
                        <ChevronsUpDownIcon data-icon="inline-end" />
                      </Button>
                    </PopoverTrigger>
                    <PopoverContent
                      align="start"
                      className="w-(--radix-popover-trigger-width) p-0"
                    >
                      <Command>
                        <CommandInput placeholder="Search providers..." />
                        <CommandList>
                          <CommandEmpty>No provider found.</CommandEmpty>
                          <CommandGroup>
                            {catalog.map((provider) => (
                              <CommandItem
                                key={provider.id}
                                value={provider.id}
                                keywords={[provider.name]}
                                data-checked={
                                  provider.id === selectedProviderId
                                }
                                onSelect={() => {
                                  setSelectedProviderId(provider.id)
                                  setComboboxOpen(false)
                                }}
                              >
                                <ProviderIcon
                                  provider={provider.id}
                                  name={provider.name}
                                  compact
                                />
                                <span>{provider.name}</span>
                              </CommandItem>
                            ))}
                          </CommandGroup>
                        </CommandList>
                      </Command>
                    </PopoverContent>
                  </Popover>
                )}
              </Field>
              {editingAccount && (
                <Field>
                  <FieldLabel>Workspace</FieldLabel>
                  <div className="flex h-8 items-center rounded-lg border px-2.5 text-sm">
                    {editingAccount.workspace_name}
                  </div>
                </Field>
              )}
              <Field>
                <FieldLabel htmlFor="provider-name">Account name</FieldLabel>
                <Input
                  id="provider-name"
                  value={displayName}
                  onChange={(event) => setDisplayName(event.target.value)}
                  placeholder="Production"
                  maxLength={80}
                  required
                />
                <FieldDescription>
                  A stable name for this provider account.
                </FieldDescription>
              </Field>
              {!editingAccount && (
                <>
                  <Field>
                    <FieldLabel htmlFor="provider-secret">
                      {selectedProvider?.credential_label ?? "Credential"}
                    </FieldLabel>
                    <Input
                      id="provider-secret"
                      type="password"
                      value={secret}
                      onChange={(event) => setSecret(event.target.value)}
                      placeholder={selectedProvider?.credential_placeholder}
                      autoComplete="off"
                      required
                    />
                    <FieldDescription>
                      Bella validates this credential automatically, then
                      encrypts it with AES-256-GCM before database storage.
                    </FieldDescription>
                  </Field>
                  {selectedProvider && (
                    <Field>
                      <FieldDescription>
                        <a
                          href={selectedProvider.docs_url}
                          target="_blank"
                          rel="noreferrer"
                        >
                          Open provider credential documentation
                          <ExternalLinkIcon className="ml-1 inline size-3" />
                        </a>
                      </FieldDescription>
                    </Field>
                  )}
                </>
              )}
              {error && (
                <Alert variant="destructive">
                  <AlertDescription>{error}</AlertDescription>
                </Alert>
              )}
            </FieldGroup>
            <SheetFooter>
              <Button type="submit" disabled={saving || !selectedProvider}>
                {saving && <Spinner data-icon="inline-start" />}
                {saving
                  ? editingAccount
                    ? "Saving"
                    : "Connecting"
                  : editingAccount
                    ? "Save changes"
                    : "Connect provider"}
              </Button>
            </SheetFooter>
          </form>
        </SheetContent>
      </Sheet>

      <AlertDialog
        open={accountToDelete !== null}
        onOpenChange={(nextOpen) => {
          if (!nextOpen && !deleting) setAccountToDelete(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete provider account?</AlertDialogTitle>
            <AlertDialogDescription>
              This permanently deletes {accountToDelete?.display_name} and its
              encrypted admin credential. This action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleting}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={deleting}
              onClick={(event) => {
                event.preventDefault()
                void handleDelete()
              }}
            >
              {deleting && <Spinner data-icon="inline-start" />}
              {deleting ? "Deleting" : "Delete account"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

function formatDateTime(value: string | null) {
  if (!value) return "never"
  return new Date(value).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  })
}
