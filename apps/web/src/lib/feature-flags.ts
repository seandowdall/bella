"use client"

import { useEffect, useState } from "react"
import posthog from "posthog-js"

export const AI_COST_VISIBILITY_FLAG = "ai-cost-visibility"

export function useAiCostVisibilityFlag() {
  const [state, setState] = useState({ enabled: false, loaded: false })

  useEffect(() => {
    const updateFlagState = () => {
      setState({
        enabled: posthog.isFeatureEnabled(AI_COST_VISIBILITY_FLAG) === true,
        loaded: true,
      })
    }

    const unsubscribe = posthog.onFeatureFlags(updateFlagState)
    updateFlagState()

    return unsubscribe
  }, [])

  return state
}
