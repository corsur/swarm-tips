import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    globals: true,
    environment: "node",
    testTimeout: 1_000_000,
    hookTimeout: 1_000_000,
    fileParallelism: false,
    setupFiles: ["./tests/vitest.setup.ts"],
    include: ["tests/**/*.ts"],
    exclude: [
      "tests/harness/**",
      "tests/helpers/**",
      "tests/fixtures/**",
      "tests/live/**",
      "tests/coordination-game/**",
      "tests/shillbot/**",
      ...(process.env.ANCHOR_PROVIDER_URL
        ? []
        : [
            "tests/coordination-game.ts",
            "tests/shillbot.ts",
            "tests/xchain.ts",
          ]),
      "tests/vitest.setup.ts",
      "**/node_modules/**",
      "**/*.d.ts",
    ],
  },
});
