"use client";

import { FormEvent, useEffect, useState } from "react";
import {
  Building2Icon,
  MessageSquareIcon,
  MoreHorizontalIcon,
  PlusIcon,
  UserIcon,
} from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  deleteAgentLlmSettings,
  getAgentLlmSettings,
  saveAgentLlmSettings,
  setDefaultAgentLlmSettings,
  sendSlackTestMessage,
} from "@/lib/api";
import type { AgentLlmSettings, Organization, User } from "@/lib/dashboard-types";

const llmModels: Record<AgentLlmSettings["provider"], string[]> = {
  openai: [
    "gpt-5.5",
    "gpt-5.5-pro",
    "gpt-5.4",
    "gpt-5.4-mini",
    "gpt-5.4-pro",
    "gpt-5.3-chat-latest",
    "gpt-5.2-chat-latest",
    "gpt-5.2",
    "gpt-5.2-pro",
    "gpt-5.1-chat-latest",
    "gpt-5.1",
    "gpt-5",
    "gpt-4.1",
    "gpt-4.1-mini",
    "gpt-4o",
    "gpt-4o-mini",
  ],
  anthropic: [
    "claude-sonnet-4-6",
    "claude-sonnet-4-5",
    "claude-sonnet-4-20250514",
    "claude-opus-4-8",
    "claude-opus-4-7",
    "claude-opus-4-6",
    "claude-opus-4-5",
    "claude-opus-4-1",
    "claude-opus-4-20250514",
  ],
};

export function SettingsPageHeader({ title, description }: { title: string; description: string }) {
  return (
    <div className="flex flex-col gap-2">
      <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
      <p className="text-muted-foreground">{description}</p>
    </div>
  );
}

export function ProfileSettings({ user }: { user: User }) {
  const displayName = user.name ?? user.github_login;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <UserIcon />
          Profile
        </CardTitle>
        <CardDescription>
          Bella uses your GitHub identity for authentication and audit trails.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="profile-name">Display name</FieldLabel>
            <Input id="profile-name" value={displayName} readOnly />
          </Field>
          <Field>
            <FieldLabel htmlFor="profile-github">GitHub login</FieldLabel>
            <Input id="profile-github" value={`@${user.github_login}`} readOnly />
            <FieldDescription>
              Profile editing will stay with your connected identity provider.
            </FieldDescription>
          </Field>
        </FieldGroup>
      </CardContent>
    </Card>
  );
}

export function OrganizationSettings({ organization }: { organization?: Organization }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Building2Icon />
          Organization
        </CardTitle>
        <CardDescription>
          Current workspace context for provider credentials, imports, and agent answers.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="organization-name">Organization</FieldLabel>
            <Input
              id="organization-name"
              value={organization?.name ?? "No organization selected"}
              readOnly
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="workspace-name">Default workspace</FieldLabel>
            <Input
              id="workspace-name"
              value={organization?.default_workspace.name ?? "None"}
              readOnly
            />
            <FieldDescription>
              Provider-reported data and future agent tools remain scoped to this organization.
            </FieldDescription>
          </Field>
        </FieldGroup>
      </CardContent>
    </Card>
  );
}

export function SlackSettings({
  organizationId,
  canManage,
}: {
  organizationId?: string;
  canManage: boolean;
}) {
  const [sending, setSending] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");

  const handleSendTestMessage = async () => {
    if (!organizationId) return;
    setSending(true);
    setError("");
    setMessage("");
    try {
      const result = await sendSlackTestMessage(organizationId);
      setMessage(`Test message sent to Slack channel ${result.channel_id}.`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not send the Slack test message.");
    } finally {
      setSending(false);
    }
  };

  return (
    <Card>
      <CardHeader>
        <div className="flex items-start justify-between gap-4">
          <div className="flex flex-col gap-1.5">
            <CardTitle className="flex items-center gap-2">
              <MessageSquareIcon />
              Slack
            </CardTitle>
            <CardDescription>
              Verify that Bella can post incident handoffs to the configured Slack channel.
            </CardDescription>
          </div>
          <Button
            type="button"
            size="sm"
            disabled={!canManage || !organizationId || sending}
            onClick={() => void handleSendTestMessage()}
          >
            {sending && <Spinner data-icon="inline-start" />}
            {sending ? "Sending" : "Send test message"}
          </Button>
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <p className="text-muted-foreground text-sm">
          Bella reads the bot token and destination channel from the local server configuration.
          This test does not expose or store Slack credentials in the dashboard.
        </p>
        {error && (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}
        {message && (
          <Alert>
            <AlertDescription>{message}</AlertDescription>
          </Alert>
        )}
        {!canManage && (
          <Alert>
            <AlertDescription>
              Organization owner or admin access is required to send a Slack test message.
            </AlertDescription>
          </Alert>
        )}
      </CardContent>
    </Card>
  );
}

export function ByokSettings({
  organizationId,
  canManage,
}: {
  organizationId?: string;
  canManage: boolean;
}) {
  const [provider, setProvider] = useState<AgentLlmSettings["provider"]>();
  const [model, setModel] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [items, setItems] = useState<AgentLlmSettings[]>([]);
  const [editingId, setEditingId] = useState<string | undefined>();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [settingDefault, setSettingDefault] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const configured = items.length > 0;
  const editing = items.find((item) => item.id === editingId);

  const setProviderAndDefaultModel = (value: AgentLlmSettings["provider"]) => {
    setProvider(value);
    setModel("");
  };

  const resetForm = () => {
    setEditingId(undefined);
    setProvider(undefined);
    setModel("");
    setDisplayName("");
    setApiKey("");
  };

  const loadSettings = async () => {
    if (!organizationId) return;
    setLoading(true);
    setError("");
    try {
      const settings = await getAgentLlmSettings(organizationId);
      setItems(settings.items ?? []);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load AI settings.");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!organizationId) return;
    const load = async () => {
      setLoading(true);
      setError("");
      try {
        const settings = await getAgentLlmSettings(organizationId);
        setItems(settings.items ?? []);
      } catch (e) {
        setError(e instanceof Error ? e.message : "Could not load AI settings.");
      } finally {
        setLoading(false);
      }
    };
    void load();
  }, [organizationId]);

  const handleSave = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!organizationId || !provider || !model) return;
    setSaving(true);
    setError("");
    setMessage("");
    try {
      const settings = await saveAgentLlmSettings({
        organizationId,
        settingId: editingId,
        displayName: displayName.trim() || model,
        provider,
        model,
        apiKey,
        isDefault: !configured || Boolean(editing?.is_default),
      });
      await loadSettings();
      resetForm();
      setDialogOpen(false);
      setMessage(`${settings.display_name} saved.`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save AI settings.");
    } finally {
      setSaving(false);
    }
  };

  const handleRemove = async (settingId: string) => {
    if (!organizationId) return;
    setRemoving(true);
    setError("");
    setMessage("");
    try {
      await deleteAgentLlmSettings(organizationId, settingId);
      await loadSettings();
      if (editingId === settingId) resetForm();
      setMessage("BYOK model removed.");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not remove AI settings.");
    } finally {
      setRemoving(false);
    }
  };

  const handleSetDefault = async (settingId: string) => {
    if (!organizationId) return;
    setSettingDefault(true);
    setError("");
    setMessage("");
    try {
      const settings = await setDefaultAgentLlmSettings(organizationId, settingId);
      await loadSettings();
      setMessage(`${settings.display_name} is now the default Bella model.`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not set default AI model.");
    } finally {
      setSettingDefault(false);
    }
  };

  const handleEdit = (item: AgentLlmSettings) => {
    setEditingId(item.id);
    setDisplayName(item.display_name);
    setProvider(item.provider);
    setModel(
      llmModels[item.provider].includes(item.model) ? item.model : llmModels[item.provider][0],
    );
    setApiKey("");
    setMessage("");
    setError("");
    setDialogOpen(true);
  };

  const handleAdd = () => {
    resetForm();
    setMessage("");
    setError("");
    setDialogOpen(true);
  };

  return (
    <Card>
      <CardHeader>
        <div className="flex items-start justify-between gap-4">
          <div className="flex flex-col gap-1.5">
            <CardTitle className="flex items-center gap-2">Bring your own key</CardTitle>
            <CardDescription>
              Configure the organization-owned LLM credentials Bella should use for the agent.
            </CardDescription>
          </div>
          <Button
            type="button"
            size="sm"
            disabled={!canManage || !organizationId}
            onClick={handleAdd}
          >
            <PlusIcon data-icon="inline-start" />
            Add key
          </Button>
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        {loading && (
          <div className="text-muted-foreground flex items-center gap-2 text-sm">
            <Spinner />
            Loading AI settings
          </div>
        )}
        {error && (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}
        {message && (
          <Alert>
            <AlertDescription>{message}</AlertDescription>
          </Alert>
        )}
        <div className="overflow-hidden rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Provider</TableHead>
                <TableHead>Model</TableHead>
                <TableHead>Key</TableHead>
                <TableHead>Status</TableHead>
                <TableHead className="w-12">
                  <span className="sr-only">Actions</span>
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {items.length ? (
                items.map((item) => (
                  <TableRow key={item.id}>
                    <TableCell className="font-medium">{item.display_name}</TableCell>
                    <TableCell>{item.provider}</TableCell>
                    <TableCell>{item.model}</TableCell>
                    <TableCell>{item.credential_fingerprint}</TableCell>
                    <TableCell>
                      {item.is_default ? (
                        <Badge variant="secondary">Default</Badge>
                      ) : (
                        <span className="text-muted-foreground">Available</span>
                      )}
                    </TableCell>
                    <TableCell className="text-right">
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild>
                          <Button
                            type="button"
                            variant="ghost"
                            size="icon-sm"
                            disabled={!canManage || saving || removing || settingDefault}
                          >
                            <MoreHorizontalIcon />
                            <span className="sr-only">Open actions for {item.display_name}</span>
                          </Button>
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                          <DropdownMenuLabel>Actions</DropdownMenuLabel>
                          <DropdownMenuGroup>
                            {!item.is_default && (
                              <DropdownMenuItem
                                disabled={settingDefault}
                                onSelect={() => void handleSetDefault(item.id)}
                              >
                                Make default
                              </DropdownMenuItem>
                            )}
                            <DropdownMenuItem onSelect={() => handleEdit(item)}>
                              Edit
                            </DropdownMenuItem>
                          </DropdownMenuGroup>
                          <DropdownMenuSeparator />
                          <DropdownMenuGroup>
                            <DropdownMenuItem
                              variant="destructive"
                              disabled={removing}
                              onSelect={() => void handleRemove(item.id)}
                            >
                              Remove
                            </DropdownMenuItem>
                          </DropdownMenuGroup>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </TableCell>
                  </TableRow>
                ))
              ) : (
                <TableRow>
                  <TableCell colSpan={6} className="h-24 text-center">
                    No keys configured yet.
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </div>
        {!canManage && (
          <Alert>
            <AlertDescription>
              Organization owner or admin access is required to edit BYOK settings.
            </AlertDescription>
          </Alert>
        )}
      </CardContent>
      <Dialog
        open={dialogOpen}
        onOpenChange={(open) => {
          setDialogOpen(open);
          if (!open) resetForm();
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{editing ? "Edit key" : "Add key"}</DialogTitle>
            <DialogDescription>
              Choose a provider, select the model Bella should call, then paste the key.
            </DialogDescription>
          </DialogHeader>
          <form onSubmit={handleSave} className="flex flex-col gap-4">
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="llm-provider">Provider</FieldLabel>
                <Select
                  value={provider}
                  onValueChange={(value) =>
                    setProviderAndDefaultModel(value as AgentLlmSettings["provider"])
                  }
                  disabled={!canManage || saving || removing}
                >
                  <SelectTrigger id="llm-provider">
                    <SelectValue placeholder="Choose provider" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      <SelectItem value="openai">OpenAI</SelectItem>
                      <SelectItem value="anthropic">Anthropic</SelectItem>
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
              <Field>
                <FieldLabel htmlFor="llm-model">Model</FieldLabel>
                <Select
                  value={model}
                  onValueChange={setModel}
                  disabled={!canManage || saving || removing || !provider}
                >
                  <SelectTrigger id="llm-model">
                    <SelectValue placeholder="Choose model" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {provider &&
                        llmModels[provider].map((modelName) => (
                          <SelectItem key={modelName} value={modelName}>
                            {modelName}
                          </SelectItem>
                        ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
                <FieldDescription>
                  API keys authenticate the provider account; Bella still needs the model to call.
                </FieldDescription>
              </Field>
              <Field>
                <FieldLabel htmlFor="llm-api-key">API key</FieldLabel>
                <Input
                  id="llm-api-key"
                  type="password"
                  value={apiKey}
                  placeholder="sk-..."
                  disabled={!canManage || saving || removing}
                  onChange={(event) => setApiKey(event.target.value)}
                  required={!editing}
                />
                <FieldDescription>
                  Leave blank while editing to keep the existing key.
                </FieldDescription>
              </Field>
              <Field>
                <FieldLabel htmlFor="llm-display-name">Name</FieldLabel>
                <Input
                  id="llm-display-name"
                  value={displayName}
                  placeholder={model || "Optional"}
                  disabled={!canManage || saving || removing}
                  onChange={(event) => setDisplayName(event.target.value)}
                />
                <FieldDescription>Optional. If empty, Bella shows the model name.</FieldDescription>
              </Field>
            </FieldGroup>
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                disabled={saving}
                onClick={() => setDialogOpen(false)}
              >
                Cancel
              </Button>
              <Button
                disabled={
                  !canManage || saving || removing || !organizationId || !provider || !model
                }
              >
                {saving && <Spinner data-icon="inline-start" />}
                {editing ? "Save changes" : "Add key"}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
