const path = require("path");
const { defineConfig } = require("@playwright/test");

const repoRoot = __dirname;
const daemonPort = 42791;

module.exports = defineConfig({
  testDir: path.join(repoRoot, "tests", "dashboard-e2e"),
  timeout: 60_000,
  expect: {
    timeout: 10_000,
  },
  fullyParallel: false,
  workers: 1,
  reporter: [["list"]],
  use: {
    baseURL: `http://127.0.0.1:${daemonPort}`,
    headless: true,
    trace: "on-first-retry",
  },
  webServer: {
    command: "node tests/dashboard-e2e/coordinator.cjs",
    url: `http://127.0.0.1:${daemonPort}/ui`,
    timeout: 180_000,
    reuseExistingServer: false,
  },
});
