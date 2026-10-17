// Hosted Windows only: interrupt a real CS2 import and prove that the same
// durable queue job resumes after the Tauri process starts again.
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { chromium } = require('playwright-core');

const temporary = process.env.RUNNER_TEMP || os.tmpdir();
const sourcePath = path.join(temporary, 'ancient', 'public-cs2-fixture.dem');
const outputDir = path.join(temporary, 'demostrike-queue-restart');
const appPath = path.resolve(__dirname, '../src-tauri/target/release/demostrike-rust-candidate.exe');
const cdpUrl = 'http://127.0.0.1:19375';
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const sha256 = (file) => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');

async function waitForPortFree() {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try {
      const response = await fetch(`${cdpUrl}/json/version`, { signal: AbortSignal.timeout(500) });
      if (!response.ok) return;
    } catch { return; }
    await sleep(500);
  }
  throw new Error('Prior native app still owns the WebView2 debugging endpoint');
}

async function launch() {
  await waitForPortFree();
  const env = { ...process.env };
  delete env.DEMOSTRIKE_SYNTHETIC_RAW_FACTS;
  const app = spawn(appPath, [], { cwd: path.dirname(appPath), windowsHide: false, stdio: 'pipe', env });
  let exited = false;
  app.once('exit', () => { exited = true; });
  app.stdout.on('data', (data) => process.stdout.write(data));
  app.stderr.on('data', (data) => process.stderr.write(data));
  let browser;
  try {
    let ready = false;
    for (let attempt = 0; attempt < 90; attempt += 1) {
      if (exited) throw new Error('Native app exited before WebView2 opened');
      try {
        const response = await fetch(`${cdpUrl}/json/version`, { signal: AbortSignal.timeout(1500) });
        if (response.ok) { ready = true; break; }
      } catch { /* Startup continues. */ }
      await sleep(1000);
    }
    assert.ok(ready, 'Native WebView2 debugging endpoint did not open');
    browser = await chromium.connectOverCDP(cdpUrl, { timeout: 15000 });
    for (let attempt = 0; attempt < 30; attempt += 1) {
      for (const context of browser.contexts()) {
        for (const page of context.pages()) {
          try {
            if (await page.evaluate(() => Boolean(window.__TAURI_INTERNALS__))) {
              return { app, browser, page };
            }
          } catch { /* Page is navigating. */ }
        }
      }
      await sleep(1000);
    }
    throw new Error('No Tauri IPC-enabled WebView2 page appeared');
  } catch (error) {
    if (browser) await browser.close().catch(() => {});
    if (!exited) app.kill();
    throw error;
  }
}

async function stop(runtime) {
  if (!runtime) return;
  const { app, browser } = runtime;
  // Terminate the process while the native worker is running. The next
  // process must recover the same row from its on-disk queue database.
  if (app.exitCode === null && app.signalCode === null) {
    const exited = new Promise((resolve) => app.once('exit', resolve));
    app.kill();
    await Promise.race([
      exited,
      sleep(15000).then(() => { throw new Error('Native app did not exit after interruption'); }),
    ]);
  }
  await browser.close().catch(() => {});
  await waitForPortFree();
}

async function jobs(page) {
  return page.evaluate(() => window.__TAURI_INTERNALS__.invoke('csdm_list_import_jobs'));
}

async function waitForJob(page, id, wanted, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const job = (await jobs(page)).find((item) => item.id === id);
    if (job?.state === wanted) return job;
    if (job?.state === 'failed') throw new Error(`Import job ${id} failed: ${job.error}`);
    await sleep(100);
  }
  throw new Error(`Import job ${id} did not reach ${wanted} within ${timeoutMs}ms`);
}

async function main() {
  assert.equal(process.platform, 'win32');
  assert.equal(process.env.EXPECT_FEATURE_FREE, '1');
  assert.ok(fs.existsSync(appPath));
  assert.equal(fs.statSync(sourcePath).size, 112473210);
  const sourceHash = sha256(sourcePath);
  assert.equal(sourceHash, 'b29a9cb537a181deef97b15cfed10ee722a37999644a27bb2226fdd77a1337fc');
  fs.mkdirSync(outputDir, { recursive: true });
  let first;
  let second;
  try {
    first = await launch();
    const before = await jobs(first.page);
    assert.equal(before.filter((job) => job.demoPath === sourcePath).length, 0,
      'Ancient fixture was already imported in this fresh hosted run');
    const created = await first.page.evaluate((demoPath) =>
      window.__TAURI_INTERNALS__.invoke('csdm_enqueue_demo', { demoPath }), sourcePath);
    assert.equal(created.demoPath, sourcePath);
    assert.equal(created.recovered, false);
    const running = await waitForJob(first.page, created.id, 'running', 30000);
    await sleep(750);
    assert.equal((await jobs(first.page)).find((job) => job.id === created.id)?.state, 'running',
      'Real parser finished before interruption could be tested');
    await stop(first);
    first = null;

    second = await launch();
    const completed = await waitForJob(second.page, created.id, 'completed', 180000);
    assert.equal(completed.recovered, true, 'The original running job was not recovered');
    assert.ok(completed.receipt, 'Recovered job has no import receipt');
    assert.equal(completed.receipt.roundCount, 8);
    const matchingJobs = (await jobs(second.page)).filter((job) => job.demoPath === sourcePath);
    assert.equal(matchingJobs.length, 1, 'Restart created a duplicate queue request');
    const snapshot = await second.page.evaluate((libraryPath) =>
      window.__TAURI_INTERNALS__.invoke('list_matches', { path: libraryPath }), completed.receipt.path);
    assert.equal(snapshot.matches.filter((row) => row.id === completed.receipt.matchId).length, 1,
      'Recovered import is missing from the saved library');
    assert.equal(snapshot.matches.filter((row) => row.demoFilePath === sourcePath).length, 1,
      'Interrupted source has multiple saved matches');
    await second.page.evaluate(() => { location.hash = '#/matches'; });
    // The queue IPC may complete before its 1.2-second UI poll and library refresh.
    // Capture only after the visible status and saved match row agree with IPC.
    await second.page.getByRole('status').filter({ hasText: /Imported.*resumed after restart/ })
      .waitFor({ timeout: 30000 });
    await second.page.getByRole('row').filter({ hasText: 'de_ancient' })
      .waitFor({ timeout: 30000 });
    const screenshot = path.join(outputDir, 'queue-recovered-native.png');
    await second.page.screenshot({ path: screenshot, animations: 'disabled' });
    const preview = await second.page.screenshot({ type: 'jpeg', quality: 42, animations: 'disabled' });
    const encoded = preview.toString('base64');
    console.log(`APP_PREVIEW_BEGIN queue-recovered-native ${preview.length}`);
    for (let offset = 0; offset < encoded.length; offset += 4096) {
      console.log(`APP_PREVIEW_CHUNK queue-recovered-native ${offset / 4096} ${encoded.slice(offset, offset + 4096)}`);
    }
    console.log('APP_PREVIEW_END queue-recovered-native');
    assert.equal(sha256(sourcePath), sourceHash, 'Interrupted import changed the source demo');
    const proof = {
      source: 'actual hosted Tauri WebView2 process interrupted during pinned Ancient import',
      fixtureSha256: sourceHash,
      fixtureBytes: fs.statSync(sourcePath).size,
      commit: process.env.GITHUB_SHA || null,
      executableSha256: sha256(appPath),
      jobId: created.id,
      beforeInterruption: running.state,
      afterRestart: completed.state,
      recovered: completed.recovered,
      receipt: completed.receipt,
      screenshotSha256: sha256(screenshot),
    };
    fs.writeFileSync(path.join(outputDir, 'proof.json'), `${JSON.stringify(proof, null, 2)}\n`);
    console.log('PASS: running Ancient import recovered as the same completed job after native process restart');
  } finally {
    await stop(first);
    await stop(second);
  }
}

main().catch((error) => { console.error(error); process.exitCode = 1; });

