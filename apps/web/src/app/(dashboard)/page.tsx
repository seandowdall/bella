"use client";

import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  AssistantRuntimeProvider,
  useLocalRuntime,
  type ChatModelAdapter,
  type ThreadMessage,
} from "@assistant-ui/react";
import { BotIcon } from "lucide-react";
import { ModelSelector, type ModelOption } from "@/components/assistant-ui/model-selector";
import { Thread } from "@/components/assistant-ui/thread";
import { getAgentLlmSettings, sendAgentMessage } from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import type { AgentLlmSettings } from "@/lib/dashboard-types";
import { useAiCostVisibilityFlag } from "@/lib/feature-flags";

const suggestions = [
  { prompt: "Summarize AI spend for the last 30 days" },
  { prompt: "Break down spend by provider" },
  { prompt: "Show model usage trends" },
  { prompt: "Check provider sync freshness" },
];

const slashCommands: Record<string, string> = {
  "/summary": "Summarize AI spend for the last 30 days",
  "/overview": "Summarize AI spend for the last 30 days",
  "/providers": "Break down spend by provider",
  "/models": "Break down spend and usage by model",
  "/sync": "Check provider sync freshness",
  "/help":
    "Show the Bella chat commands for spend, provider usage, model breakdowns, and sync freshness",
};

export default function HomePage() {
  const { selectedOrganization } = useAuth();
  const { enabled: costVisibilityEnabled } = useAiCostVisibilityFlag();
  const [models, setModels] = useState<AgentLlmSettings[]>([]);
  const [modelId, setModelId] = useState<string>();
  const [modelError, setModelError] = useState("");

  useEffect(() => {
    if (!selectedOrganization) return;
    let cancelled = false;

    const loadModels = async () => {
      try {
        const settings = await getAgentLlmSettings(selectedOrganization.id);
        if (cancelled) return;
        setModels(settings.items);
        setModelId((current) => current ?? settings.default_id ?? undefined);
      } catch (error) {
        if (cancelled) return;
        setModelError(error instanceof Error ? error.message : "Could not load AI settings.");
      }
    };

    void loadModels();

    return () => {
      cancelled = true;
    };
  }, [selectedOrganization]);

  const modelOptions = useMemo<ModelOption[]>(
    () =>
      models.map((model) => ({
        id: model.id,
        name: model.display_name,
        description: `${model.provider} ${model.model}${model.is_default ? " default" : ""}`,
        icon: <BotIcon />,
        keywords: [model.provider, model.model],
      })),
    [models],
  );

  return (
    <BellaRuntimeProvider
      organizationId={selectedOrganization?.id}
      costVisibilityEnabled={costVisibilityEnabled}
    >
      <div className="-m-4 flex min-h-[calc(100svh-var(--header-height))] flex-1 flex-col bg-background lg:-m-6">
        <Thread
          components={{
            ComposerAccessory: () => (
              <BellaModelSelector
                models={modelOptions}
                value={modelId}
                onValueChange={setModelId}
              />
            ),
            Welcome: () => (
              <BellaWelcome costVisibilityEnabled={costVisibilityEnabled} error={modelError} />
            ),
          }}
          costVisibilityEnabled={costVisibilityEnabled}
        />
      </div>
    </BellaRuntimeProvider>
  );
}

function BellaRuntimeProvider({
  organizationId,
  costVisibilityEnabled,
  children,
}: {
  organizationId?: string;
  costVisibilityEnabled: boolean;
  children: ReactNode;
}) {
  const adapter = useMemo<ChatModelAdapter>(
    () => ({
      async run({ messages, abortSignal, context }) {
        if (!organizationId) {
          return {
            content: [
              {
                type: "text",
                text: "Select an organization before asking Bella a question.",
              },
            ],
          };
        }

        abortSignal.throwIfAborted();
        const message = normalizePrompt(lastTextMessage(messages), costVisibilityEnabled);
        const modelName = context.config?.modelName;
        const llmSettingId = typeof modelName === "string" ? modelName : undefined;
        const response = await sendAgentMessage({
          organizationId,
          message,
          llmSettingId,
          signal: abortSignal,
        });

        return {
          content: [{ type: "text", text: response.answer }],
          metadata: {
            custom: {
              agentMode: response.agent_mode,
              freshness: response.freshness,
              metricType: response.metric_type,
              sources: response.sources,
              suggestions: response.suggestions,
            },
          },
        };
      },
    }),
    [costVisibilityEnabled, organizationId],
  );

  const runtime = useLocalRuntime(adapter, {
    adapters: {
      suggestion: {
        generate: async () => (costVisibilityEnabled ? suggestions : []),
      },
    },
  });

  return <AssistantRuntimeProvider runtime={runtime}>{children}</AssistantRuntimeProvider>;
}

function BellaModelSelector({
  models,
  value,
  onValueChange,
}: {
  models: ModelOption[];
  value?: string;
  onValueChange: (value: string) => void;
}) {
  if (!models.length) return null;

  return (
    <ModelSelector
      models={models}
      value={value}
      onValueChange={onValueChange}
      variant="ghost"
      size="sm"
      searchable
      className="text-muted-foreground hover:text-foreground"
      contentClassName="w-80"
    />
  );
}

function BellaWelcome({
  costVisibilityEnabled,
  error,
}: {
  costVisibilityEnabled: boolean;
  error?: string;
}) {
  return (
    <div className="mb-6 flex flex-col items-center px-4 text-center">
      <p className="mb-2 text-sm font-medium tracking-[0.18em] text-muted-foreground uppercase">
        Bella
      </p>
      <h1 className="text-2xl font-semibold tracking-tight">
        {costVisibilityEnabled
          ? "Ask about AI spend, models, providers, and sync health."
          : "Ask Bella to investigate incidents and operational health."}
      </h1>
      {error && <p className="mt-3 text-sm text-muted-foreground">{error}</p>}
    </div>
  );
}

function lastTextMessage(messages: readonly ThreadMessage[]) {
  const message = messages.findLast((item) => item.role === "user");
  if (!message) return "";

  return message.content
    .filter((part) => part.type === "text")
    .map((part) => part.text)
    .join("\n");
}

function normalizePrompt(prompt: string, costVisibilityEnabled: boolean) {
  const trimmed = prompt.trim();
  if (!costVisibilityEnabled) return trimmed;

  return slashCommands[trimmed.toLowerCase()] ?? trimmed;
}
