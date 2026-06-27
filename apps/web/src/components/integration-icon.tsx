import Image from "next/image";
import { CableIcon } from "lucide-react";
import { cn } from "@/lib/utils";

const integrationIcons: Record<string, string | { light: string; dark: string }> = {
  linear: "/integrations/linear.svg",
  posthog: "/integrations/posthog.svg",
  sentry: {
    light: "/integrations/sentry-dark.svg",
    dark: "/integrations/sentry.svg",
  },
  slack: "/integrations/slack.png",
};

export function IntegrationIcon({
  integration,
  name,
  compact = false,
}: {
  integration: string;
  name: string;
  compact?: boolean;
}) {
  const icon = integrationIcons[integration];

  if (!icon) {
    return (
      <div
        className={cn(
          "flex items-center justify-center rounded-lg bg-muted",
          compact ? "size-6" : "size-10",
        )}
      >
        <CableIcon />
      </div>
    );
  }

  return (
    <div
      className={cn(
        "flex items-center justify-center rounded-lg bg-muted",
        compact ? "size-6 p-1" : "size-10 p-2",
      )}
    >
      {typeof icon === "string" ? (
        <Image
          src={icon}
          alt={`${name} logo`}
          width={compact ? 24 : 40}
          height={compact ? 24 : 40}
          className="size-full object-contain"
        />
      ) : (
        <>
          <Image
            src={icon.light}
            alt={`${name} logo`}
            width={compact ? 24 : 40}
            height={compact ? 24 : 40}
            className="size-full object-contain dark:hidden"
          />
          <Image
            src={icon.dark}
            alt={`${name} logo`}
            width={compact ? 24 : 40}
            height={compact ? 24 : 40}
            className="hidden size-full object-contain dark:block"
          />
        </>
      )}
    </div>
  );
}
