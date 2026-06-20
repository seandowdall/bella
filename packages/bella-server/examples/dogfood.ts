import { createBellaServerFromEnv } from "../src/index.js";

const bella = createBellaServerFromEnv();

export async function runDogfoodExample<T>(call: () => Promise<T>): Promise<T> {
  if (!bella) {
    return call();
  }

  return bella.trackLlmCall({
    operation: "bella.dogfood",
    call,
    metadata: {
      service: "bella-hosted",
    },
  });
}
