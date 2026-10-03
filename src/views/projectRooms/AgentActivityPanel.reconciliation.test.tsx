/// <reference types="node" />
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { build } from 'esbuild';
import { expect, it } from 'vitest';

// Uses installed Chromium instead of adding a DOM emulator/test dependency.
// Set TUNNELDOCK_TEST_BROWSER when Chromium is outside these standard locations.
it('keeps mounted campaign/context panels unique and project-scoped without leaking effects', async () => {
  const browser = [
    process.env.TUNNELDOCK_TEST_BROWSER,
    'C:/Program Files/Google/Chrome/Application/chrome.exe',
    'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
    '/usr/bin/chromium', '/usr/bin/chromium-browser', '/usr/bin/google-chrome',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  ].find((path): path is string => !!path && existsSync(path));
  if (!browser) throw new Error('Mounted DOM regression requires installed Chromium; set TUNNELDOCK_TEST_BROWSER.');
  const directory = mkdtempSync(join(tmpdir(), 'tunneldock-reconciliation-'));
  try {
    const bundle = await build({
      entryPoints: [join(dirname(fileURLToPath(import.meta.url)), 'AgentActivityPanel.reconciliation.fixture.tsx')],
      bundle: true, write: false, format: 'iife', platform: 'browser', jsx: 'automatic',
      define: { 'process.env.NODE_ENV': '"development"' },
    });
    const html = join(directory, 'fixture.html');
    writeFileSync(join(directory, 'fixture.js'), bundle.outputFiles[0].text);
    writeFileSync(html, '<!doctype html><html><body><script src="fixture.js"></script></body></html>');
    const output = execFileSync(browser, [
      '--headless', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
      `--user-data-dir=${join(directory, 'profile')}`, '--dump-dom', pathToFileURL(html).href,
    ], { encoding: 'utf8', timeout: 45_000, maxBuffer: 4 * 1024 * 1024, windowsHide: true });
    expect(output).toContain('<pre id="reconciliation-result">PASS</pre>');
  } finally {
    rmSync(directory, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  }
}, 60_000);
