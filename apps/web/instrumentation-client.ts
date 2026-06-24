import posthog from "posthog-js"

const trackingDisabledHosts = new Set([
  "localhost",
  "127.0.0.1",
  "app.qa.bella.md",
])

const trackingDisabled = trackingDisabledHosts.has(window.location.hostname)

posthog.init(process.env.NEXT_PUBLIC_POSTHOG_PROJECT_TOKEN!, {
  api_host: process.env.NEXT_PUBLIC_POSTHOG_HOST,
  ui_host: "https://eu.posthog.com",
  defaults: "2026-01-30",
  autocapture: !trackingDisabled,
  capture_exceptions: !trackingDisabled && process.env.NODE_ENV !== "development",
  capture_pageview: !trackingDisabled,
  disable_session_recording: trackingDisabled,
  opt_out_capturing_by_default: trackingDisabled,
  opt_out_persistence_by_default: trackingDisabled,
  debug: !trackingDisabled && process.env.NODE_ENV === "development",
})
