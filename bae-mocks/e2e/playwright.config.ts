import { defineConfig, devices } from '@playwright/test';

const isCI = !!process.env.CI;

export default defineConfig({
  testDir: '.',
  fullyParallel: false,
  forbidOnly: isCI,
  retries: isCI ? 2 : 0,
  timeout: 60000,
  workers: 1,
  reporter: 'list',
  use: {
    baseURL: 'http://localhost:8080',
    trace: 'on-first-retry',
    viewport: { width: 1400, height: 900 },
    navigationTimeout: isCI ? 60000 : 30000,
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: {
    command: `cd .. && dx serve --release --port 8080`,
    url: 'http://localhost:8080',
    reuseExistingServer: !isCI,
    timeout: isCI ? 300000 : 120000,
  },
});
