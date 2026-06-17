"use client"

import { FormEvent, useState } from "react"
import { ArrowRightIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { Spinner } from "@/components/ui/spinner"
import { sendAgentMessage } from "@/lib/api"
import { useAuth } from "@/lib/auth-context"
import type { AgentMessage } from "@/lib/dashboard-types"

const initialMessage: AgentMessage = {
  role: "assistant",
  content:
    "Ask me about AI spend, provider usage, model breakdowns, and sync freshness.",
  metric_type: "provider_reported",
  sources: ["cost_snapshots", "usage_buckets", "provider_accounts"],
}

export default function HomePage() {
  const { selectedOrganization } = useAuth()
  const [input, setInput] = useState("")
  const [messages, setMessages] = useState<AgentMessage[]>([initialMessage])
  const [sending, setSending] = useState(false)

  const askBella = async (message: string) => {
    if (!selectedOrganization || !message.trim()) return
    const userMessage: AgentMessage = {
      role: "user",
      content: message.trim(),
    }
    setMessages((current) => [...current, userMessage])
    setInput("")
    setSending(true)
    try {
      const response = await sendAgentMessage({
        organizationId: selectedOrganization.id,
        message: userMessage.content,
      })
      setMessages((current) => [
        ...current,
        {
          role: "assistant",
          content: response.answer,
          freshness: response.freshness,
          metric_type: response.metric_type,
          sources: response.sources,
          suggestions: response.suggestions,
        },
      ])
    } catch (e) {
      setMessages((current) => [
        ...current,
        {
          role: "assistant",
          content:
            e instanceof Error
              ? e.message
              : "Bella could not reach the agent API. Restart the API on this branch and try again.",
        },
      ])
    } finally {
      setSending(false)
    }
  }

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    void askBella(input)
  }

  return (
    <div className="-m-4 flex min-h-[calc(100svh-var(--header-height))] flex-1 flex-col bg-black px-4 py-6 lg:-m-6 lg:px-6">
      <div className="mx-auto flex w-full max-w-4xl flex-1 flex-col">
        <div className="flex flex-1 flex-col gap-4 overflow-y-auto pb-6">
          {messages.map((message, index) => (
            <MessageBubble key={index} message={message} />
          ))}
          {sending && (
            <div className="flex items-center gap-2 text-sm text-zinc-500">
              <Spinner />
              Bella is checking provider data...
            </div>
          )}
        </div>
        <form
          onSubmit={handleSubmit}
          className="sticky bottom-0 flex rounded-2xl border border-zinc-700 bg-zinc-950/95 p-2 shadow-2xl shadow-black/50 focus-within:border-primary"
        >
          <Textarea
            value={input}
            onChange={(event) => setInput(event.target.value)}
            placeholder="Ask Bella about AI spend, models, providers, or sync health..."
            className="min-h-14 flex-1 resize-none border-0 bg-transparent px-3 py-4 text-base text-white shadow-none outline-none placeholder:text-zinc-500 focus-visible:ring-0"
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault()
                void askBella(input)
              }
            }}
          />
          <Button
            type="submit"
            disabled={sending || !input.trim()}
            className="h-auto min-h-14 w-14 self-stretch rounded-xl"
            aria-label="Send message"
          >
            {sending ? <Spinner /> : <ArrowRightIcon />}
          </Button>
        </form>
      </div>
    </div>
  )
}

function MessageBubble({ message }: { message: AgentMessage }) {
  const isAssistant = message.role === "assistant"
  return (
    <div className={isAssistant ? "flex justify-start" : "flex justify-end"}>
      <div
        className={
          isAssistant
            ? "max-w-[80%] rounded-2xl border border-zinc-800 bg-zinc-950 px-4 py-3 text-zinc-100"
            : "max-w-[80%] rounded-2xl bg-white px-4 py-3 text-black"
        }
      >
        <p className="whitespace-pre-wrap text-sm leading-6">{message.content}</p>
        {message.freshness && (
          <p className="mt-2 text-xs text-zinc-500">{message.freshness}</p>
        )}
      </div>
    </div>
  )
}
