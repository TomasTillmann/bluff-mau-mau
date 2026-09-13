const { defineConfig } = require('@playwright/test');
const path = require('node:path');
const os = require('node:os');

module.exports = defineConfig({
  testDir: __dirname,
  testMatch: 'smoke.spec.cjs',
  workers: 1,
  retries: 0,
  timeout: 15000,
  reporter: 'list',
  use: {
    baseURL: 'http://127.0.0.1:18767',
    channel: 'chrome',
    viewport: { width: 1272, height: 900 },
    reducedMotion: 'no-preference',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  webServer: {
    command: 'cargo run --release --bin server -- --port 18767',
    env: { PATH: `${process.env.PATH}${path.delimiter}${path.join(os.homedir(), '.cargo', 'bin')}` },
    cwd: path.resolve(__dirname, '../..'),
    url: 'http://127.0.0.1:18767/api/state',
    reuseExistingServer: false,
    timeout: 120000,
  },
});
