import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    exclude: ["**/node_modules/**", "**/dist/**", "**/.next/**", "**/target/**", "apps/site/**"],
    passWithNoTests: true,
  },
});
