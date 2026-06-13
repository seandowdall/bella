import Image from "next/image"
import { KeyRoundIcon } from "lucide-react"
import { cn } from "@/lib/utils"

const providerIcons: Record<string, string> = {
  anthropic: "/providers/anthropic.jpeg",
  openai: "/providers/openai.jpeg",
}

export function ProviderIcon({
  provider,
  name,
  compact = false,
}: {
  provider: string
  name: string
  compact?: boolean
}) {
  const src = providerIcons[provider]

  if (!src) {
    return (
      <div
        className={cn(
          "flex items-center justify-center rounded-lg bg-muted",
          compact ? "size-6" : "size-8",
        )}
      >
        <KeyRoundIcon />
      </div>
    )
  }

  return (
    <Image
      src={src}
      alt={`${name} logo`}
      width={compact ? 24 : 32}
      height={compact ? 24 : 32}
      className={cn(
        "rounded-lg object-cover",
        compact ? "size-6" : "size-8",
      )}
    />
  )
}
