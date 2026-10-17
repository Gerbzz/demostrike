// Hosted Windows only: exercise a pinned public CS2 demo in the actual Tauri
// WebView2 window. In the feature-free build, deliberately set the synthetic
// parser environment variable and prove the native decoder still runs.
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { chromium } = require('playwright-core');

const outputDir = path.join(process.env.RUNNER_TEMP || os.tmpdir(), 'demostrike-public-native');
const demoPath = path.join(process.env.RUNNER_TEMP || os.tmpdir(), 'public-cs2-fixture.dem');
const appPath = path.resolve(__dirname, '../src-tauri/target/release/demostrike-rust-candidate.exe');
const cdpUrl = 'http://127.0.0.1:19375';
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const sha256 = (file) => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');

async function captureAppOnly(page, file) {
  await page.screenshot({ path: file, animations: 'disabled' });
  // Small previews from the same actual WebView2 window let review inspect
  // pixels through hosted logs when artifact downloads are unavailable.
  const preview = await page.screenshot({ type: 'jpeg', quality: 42, animations: 'disabled' });
  const label = path.basename(file, '.png');
  const encoded = preview.toString('base64');
  console.log(`APP_PREVIEW_BEGIN ${label} ${preview.length}`);
  for (let offset = 0; offset < encoded.length; offset += 4096) {
    console.log(`APP_PREVIEW_CHUNK ${label} ${offset / 4096} ${encoded.slice(offset, offset + 4096)}`);
  }
  console.log(`APP_PREVIEW_END ${label}`);
}

async function main() {
  assert.equal(process.platform, 'win32');
  assert.ok(fs.existsSync(appPath), `Native app missing: ${appPath}`);
  assert.equal(fs.statSync(demoPath).size, 60601900);
  const sourceHash = sha256(demoPath);
  fs.mkdirSync(outputDir, { recursive: true });
  let portFree = false;
  for (let attempt = 0; attempt < 30; attempt += 1) {
    try {
      const old = await fetch(`${cdpUrl}/json/version`, { signal: AbortSignal.timeout(500) });
      if (old.ok) { await sleep(500); continue; }
      portFree = true;
      break;
    } catch { portFree = true; break; }
  }
  assert.ok(portFree, 'Prior native app still owns the WebView2 debugging endpoint');
  const env = { ...process.env };
  const featureFree = process.env.EXPECT_FEATURE_FREE === '1';
  if (featureFree) {
    // Deliberately request synthetic facts. A release build without the
    // feature must ignore this environment variable and decode the real demo.
    env.DEMOSTRIKE_SYNTHETIC_RAW_FACTS = path.join(__dirname, 'fixtures', 'combat-raw.json');
    assert.ok(fs.existsSync(env.DEMOSTRIKE_SYNTHETIC_RAW_FACTS));
  } else {
    delete env.DEMOSTRIKE_SYNTHETIC_RAW_FACTS;
  }
  const app = spawn(appPath, [], { cwd: path.dirname(appPath), windowsHide: false, stdio: 'pipe', env });
  let exit = null;
  app.once('exit', (code, signal) => { exit = `${code ?? 'none'} / ${signal ?? 'none'}`; });
  app.stdout.on('data', (data) => process.stdout.write(data));
  app.stderr.on('data', (data) => process.stderr.write(data));
  let browser;
  let page;
  const pageErrors = [];
  try {
    let ready = false;
    for (let attempt = 0; attempt < 90; attempt += 1) {
      if (exit !== null) throw new Error(`App exited before WebView2 opened: ${exit}`);
      try {
        const response = await fetch(`${cdpUrl}/json/version`, { signal: AbortSignal.timeout(1500) });
        if (response.ok) { ready = true; break; }
      } catch { /* Native startup still in progress. */ }
      await sleep(1000);
    }
    assert.ok(ready, 'Native WebView2 debugging endpoint did not open');
    browser = await chromium.connectOverCDP(cdpUrl, { timeout: 15000 });
    for (let attempt = 0; attempt < 30 && !page; attempt += 1) {
      for (const context of browser.contexts()) {
        for (const target of context.pages()) {
          try {
            if (await target.evaluate(() => Boolean(window.__TAURI_INTERNALS__))) page = target;
          } catch { /* Page is navigating. */ }
        }
      }
      if (!page) await sleep(1000);
    }
    assert.ok(page, 'No Tauri IPC-enabled page appeared');
    page.on('pageerror', (error) => pageErrors.push(error.message));
    await page.evaluate(() => { location.hash = '#/matches'; });
    await page.waitForFunction(() => Boolean(document.querySelector('#demo-import-path'))
      || [...document.querySelectorAll('button')].some((button) => button.textContent?.includes('Change library')),
      null, { timeout: 30000 });
    const changeLibrary = page.getByRole('button', { name: 'Change library' });
    if (await changeLibrary.isVisible().catch(() => false)) await changeLibrary.click();
    await page.getByRole('textbox', { name: 'Import a CS2 demo' }).fill(demoPath);
    await page.getByRole('button', { name: 'Import demo' }).click();
    const row = page.getByRole('row').filter({ hasText: 'public-cs2-fixture.dem' });
    await row.waitFor({ timeout: 180000 });
    assert.ok((await row.innerText()).includes('de_mirage'));
    const matchesShot = path.join(outputDir, 'real-demo-matches.png');
    await captureAppOnly(page, matchesShot);
    await row.dblclick();
    await page.getByRole('heading', { name: 'de_mirage' }).waitFor({ timeout: 30000 });
    await page.getByRole('heading', { name: 'Recorded players' }).waitFor({ timeout: 30000 });
    assert.equal(await page.getByText('Synthetic CI fixture:', { exact: false }).count(), 0);
    const nativeFacts = await page.evaluate(async () => {
      const library = localStorage.getItem('csdm-port-selected-library');
      const matchId = Number(location.hash.match(/local-(\d+)/)?.[1]);
      if (!library || !Number.isSafeInteger(matchId)) throw new Error('Local match route not available');
      const detail = await window.__TAURI_INTERNALS__.invoke('get_match_detail', { path: library, matchId });
      return {
        capability: detail.summary.parserCapability,
        roundStarts: detail.rounds.map((round) => round.startTick),
        deaths: detail.rounds.reduce((count, round) => count + (round.deaths?.length ?? 0), 0),
        hurts: detail.rounds.reduce((count, round) => count + (round.hurts?.length ?? 0), 0),
        missingStreams: detail.rounds.filter((round) => round.deaths === null || round.hurts === null).length,
        unknownEffectiveHurts: detail.rounds.flatMap((round) => round.hurts ?? [])
          .filter((hurt) => hurt.effectiveDamage === null).length,
        overkillHurts: detail.rounds.flatMap((round) => round.hurts ?? [])
          .filter((hurt) => hurt.effectiveDamage !== null && hurt.rawDamage > hurt.effectiveDamage).length,
        teamSides: detail.rounds.map((round) => [round.teamASide, round.teamBSide]),
        effectivePlayers: detail.players.filter((player) => player.combat?.effectiveEnemyHealthDamage !== null
          && player.combat?.effectiveEnemyHealthDamage !== undefined).length,
        stablePlayers: detail.players.filter((player) => player.combat?.stableTeam === 'A'
          || player.combat?.stableTeam === 'B').length,
      };
    });
    const capability = nativeFacts.capability;
    assert.equal(capability, 'native-combat-full-v2', 'Real decoder provenance was not retained');
    assert.equal(nativeFacts.roundStarts[0], 67, 'Pinned Mirage first round differs from the original analyzer');
    assert.equal(nativeFacts.deaths, 64, 'Persisted in-match deaths were not attributed to recorded rounds');
    assert.equal(nativeFacts.hurts, 264, 'Persisted raw damage event identities did not reach the native service');
    assert.equal(nativeFacts.missingStreams, 0, 'A verified round lost its raw combat stream');
    assert.equal(nativeFacts.unknownEffectiveHurts, 0, 'Native hurt stream has unavailable effective HP');
    assert.ok(nativeFacts.overkillHurts > 0, 'Raw and effective HP were incorrectly collapsed');
    assert.ok(nativeFacts.teamSides.every(([a, b]) => a === 'CT' && b === 'T'),
      'Observed pre-halftime Team A/B sides did not reach the native service');
    assert.equal(nativeFacts.effectivePlayers, 10, 'Effective player damage totals did not reach the native service');
    assert.equal(nativeFacts.stablePlayers, 10, 'Observed stable Team A/B assignment did not reach players');
    const overviewShot = path.join(outputDir, 'real-demo-overview.png');
    await captureAppOnly(page, overviewShot);
    await page.getByRole('link', { name: 'Rounds', exact: true }).click();
    await page.getByRole('heading', { name: 'Recorded rounds' }).waitFor({ timeout: 30000 });
    await page.waitForFunction(() => document.querySelectorAll('section[aria-label="Round entries"] details').length >= 10, null, { timeout: 30000 });
    const roundRows = await page.locator('section[aria-label="Round entries"] details').count();
    assert.ok(roundRows >= 10, `Unexpected round count: ${roundRows}`);
    const roundsShot = path.join(outputDir, 'real-demo-rounds.png');
    await captureAppOnly(page, roundsShot);
    await page.locator('section[aria-label="Round entries"] details').first().locator('summary').click();
    await page.getByRole('heading', { name: 'Recorded death events' }).first().waitFor({ timeout: 30000 });
    await page.getByText('All CT eliminated').first().waitFor({ timeout: 30000 });
    assert.ok(await page.locator('section[aria-label="Round entries"] ol li').count() > 0,
      'Native raw death events did not reach the partial CSDM round view');
    const roundDetailShot = path.join(outputDir, 'real-demo-round-detail.png');
    await captureAppOnly(page, roundDetailShot);
    await page.getByRole('link', { name: 'Open round 1', exact: true }).click();
    await page.getByRole('heading', { name: 'Round 1', exact: true }).waitFor({ timeout: 30000 });
    await page.getByRole('heading', { name: 'Recorded damage events' }).waitFor({ timeout: 30000 });
    assert.ok(await page.locator('section[aria-label="Round raw damage"] ol li').count() > 0,
      'Raw damage events did not reach the selected round route');
    assert.ok(await page.locator('section[aria-label="Round raw damage"] ol li').getByText(/effective HP/).count() > 0,
      'Effective HP is absent from the selected round detail');
    const selectedRoundShot = path.join(outputDir, 'real-demo-selected-round.png');
    await captureAppOnly(page, selectedRoundShot);
    await page.locator('section[aria-label="Round raw damage"] ol li').first().scrollIntoViewIfNeeded();
    const effectiveDamageShot = path.join(outputDir, 'real-demo-effective-damage.png');
    await captureAppOnly(page, effectiveDamageShot);
    await page.getByRole('link', { name: 'Next round' }).click();
    await page.getByRole('heading', { name: 'Round 2', exact: true }).waitFor({ timeout: 30000 });
    await page.evaluate(() => { location.hash = location.hash.split('/rounds/')[0] + '/rounds/999'; });
    await page.getByRole('alert').filter({ hasText: 'not recorded in this library' }).waitFor({ timeout: 30000 });
    await page.getByRole('link', { name: 'All rounds' }).click();
    await page.getByRole('heading', { name: 'Recorded rounds' }).waitFor({ timeout: 30000 });
    await page.getByRole('link', { name: 'Duels', exact: true }).click();
    await page.getByRole('heading', { name: 'Recorded duels' }).waitFor({ timeout: 30000 });
    const matrix = page.locator('section[aria-label="Observed duel matrix"]');
    const nonzero = matrix.locator('button:not([disabled])');
    assert.ok(await nonzero.count() > 0, 'No opposing-side duel pairs appeared');
    const duelsShot = path.join(outputDir, 'real-demo-duels.png');
    await captureAppOnly(page, duelsShot);
    await nonzero.first().click();
    const duelEvents = page.locator('section[aria-label="Selected duel events"] li');
    assert.ok(await duelEvents.count() > 0, 'Selecting a duel did not reveal source events');
    await duelEvents.first().scrollIntoViewIfNeeded();
    const duelEventsShot = path.join(outputDir, 'real-demo-duel-events.png');
    await captureAppOnly(page, duelEventsShot);
    await duelEvents.first().getByRole('link').click();
    await page.getByRole('heading', { name: /^Round [0-9]+$/ }).waitFor({ timeout: 30000 });
    await page.getByRole('link', { name: 'Players' }).click();
    await page.getByRole('heading', { name: 'Recorded players' }).waitFor({ timeout: 30000 });
    await page.waitForFunction(() => document.querySelectorAll('tbody tr').length >= 10, null, { timeout: 30000 });
    const playerRows = await page.locator('tbody tr').count();
    assert.ok(playerRows >= 10, `Unexpected player count: ${playerRows}`);
    await page.getByText('DMG', { exact: true }).first().waitFor({ timeout: 30000 });
    const playersShot = path.join(outputDir, 'real-demo-players.png');
    await captureAppOnly(page, playersShot);
    const firstPlayerLink = page.locator('[aria-label="Recorded player scoreboard"] tbody tr').first().getByRole('link').first();
    const firstPlayerName = (await firstPlayerLink.innerText()).trim();
    await firstPlayerLink.click();
    await page.getByRole('heading', { name: firstPlayerName, exact: true }).waitFor({ timeout: 30000 });
    await page.getByRole('heading', { name: 'Round by round' }).waitFor({ timeout: 30000 });
    const playerDetailRows = await page.locator('section[aria-label="Player rounds"] tbody tr').count();
    assert.equal(playerDetailRows, 9, 'Only nine live rounds have recorded player facts; the final surrender must stay unavailable');
    const summaryLayout = await page.evaluate(() => {
      const table = document.querySelector('table');
      const header = table?.querySelector('thead');
      const row = table?.querySelector('tbody tr');
      const headerBox = header?.getBoundingClientRect();
      const rowBox = row?.getBoundingClientRect();
      return { rowText: row?.textContent, headerBottom: headerBox?.bottom,
        rowTop: rowBox?.top, rowHeight: rowBox?.height };
    });
    console.log('PLAYER_DETAIL_SUMMARY_LAYOUT', JSON.stringify(summaryLayout));
    assert.ok(summaryLayout.rowText?.includes(firstPlayerName), 'Selected player summary row is missing');
    assert.ok(summaryLayout.rowHeight >= 20 && summaryLayout.rowTop >= summaryLayout.headerBottom - 1,
      'Selected player summary row is clipped below its header');
    const playerDetailShot = path.join(outputDir, 'real-demo-player-detail.png');
    await captureAppOnly(page, playerDetailShot);
    await page.getByRole('link', { name: 'All players' }).click();
    await page.getByRole('heading', { name: 'Recorded players' }).waitFor({ timeout: 30000 });
    await page.getByRole('link', { name: 'Utility' }).click();
    await page.getByRole('heading', { name: 'Recorded utility' }).waitFor({ timeout: 30000 });
    await page.waitForFunction(() => document.querySelectorAll('tbody tr').length === 150, null, { timeout: 30000 });
    const utilityRows = await page.locator('tbody tr').count();
    assert.equal(utilityRows, 150);
    const utilityShot = path.join(outputDir, 'real-demo-utility.png');
    await captureAppOnly(page, utilityShot);
    const radarPath = process.env.CSDM_PUBLIC_RADAR_PATH;
    assert.ok(radarPath && fs.existsSync(radarPath), 'Pinned test radar was not downloaded');
    await page.getByRole('link', { name: 'Finder' }).click();
    await page.getByRole('heading', { name: 'Grenade finder' }).waitFor({ timeout: 30000 });
    await page.waitForFunction(() => document.querySelector('[data-finder-count]')?.getAttribute('data-finder-count') === '150',
      null, { timeout: 30000 });
    const finderTarget = await page.evaluate(async () => {
      const library = localStorage.getItem('csdm-port-selected-library');
      const matchId = Number(location.hash.match(/local-(\d+)/)?.[1]);
      const throws = await window.__TAURI_INTERNALS__.invoke('get_utility_finder', { path: library, matchId });
      const detail = await window.__TAURI_INTERNALS__.invoke('get_match_detail', { path: library, matchId });
      const item = throws.find((entry) => entry.trajectory.length >= 2
        && detail.rounds.some((round) => round.roundNumber === entry.roundNumber
          && entry.throwTick >= round.startTick && entry.throwTick <= round.endTick));
      return { count: throws.length, item: item && { id: item.id, round: item.roundNumber,
        type: item.utilityType, tick: item.throwTick, points: item.trajectory.length } };
    });
    assert.equal(finderTarget.count, 150);
    assert.ok(finderTarget.item, 'Pinned public demo has no recorded finder path inside a replay round');
    await page.getByRole('combobox', { name: 'Round' }).selectOption(String(finderTarget.item.round));
    await page.getByRole('combobox', { name: 'Grenade type' }).selectOption(finderTarget.item.type);
    await page.locator(`[data-throw-id="${finderTarget.item.id}"]`).click();
    assert.equal(await page.locator('[data-selected-throw]').getAttribute('data-selected-throw'), String(finderTarget.item.id));
    assert.equal(await page.locator('[data-selected-throw] svg polyline').count(), 2, 'Selected measured path was not drawn');
    await page.getByRole('button', { name: 'Full map' }).click();
    await page.waitForFunction(() => document.querySelector('[data-selected-throw] svg[role="img"]')
      ?.getAttribute('viewBox')?.endsWith('1024 1024'));
    await page.getByRole('button', { name: 'Fit selected path' }).click();
    await page.waitForFunction(() => Number(document.querySelector('[data-selected-throw] svg[role="img"]')
      ?.getAttribute('viewBox')?.split(' ')[2]) < 1024);
    await page.getByLabel('Choose local radar PNG').setInputFiles(radarPath);
    await page.locator('svg image').waitFor({ timeout: 15000 });
    const finderShot = path.join(outputDir, 'real-demo-grenade-finder.png');
    await captureAppOnly(page, finderShot);
    await page.getByRole('spinbutton', { name: 'Lineup pause (ms)' }).fill('2000');
    await page.getByRole('button', { name: 'Create offline edit plan' }).click();
    await page.getByText('Offline plan saved locally.').waitFor({ timeout: 60000 });
    const handoffPaths = await page.evaluate(() => {
      const text = document.querySelector('[data-selected-throw] [role="status"]')?.textContent ?? '';
      const plan = text.match(/Plan:\s*(.+?capture-edit-plan\.json)/);
      const project = text.match(/Project:\s*(.+?offline-edit\.cstrike)/);
      const request = text.match(/CSthrowback request:\s*(.+?capture-request\.json)/);
      const filmcraft = text.match(/FilmCraft:\s*(.+?offline-edit\.fcproj)/);
      return { plan: plan?.[1], project: project?.[1], request: request?.[1], filmcraft: filmcraft?.[1] };
    });
    assert.ok(handoffPaths.plan && handoffPaths.project && handoffPaths.request && handoffPaths.filmcraft,
      'Local handoff paths were not shown');
    const editPlan = JSON.parse(fs.readFileSync(handoffPaths.plan, 'utf8'));
    const editProject = JSON.parse(fs.readFileSync(handoffPaths.project, 'utf8'));
    const captureRequest = JSON.parse(fs.readFileSync(handoffPaths.request, 'utf8'));
    const filmcraftFile = JSON.parse(fs.readFileSync(handoffPaths.filmcraft, 'utf8'));
    assert.equal(filmcraftFile.format, 'filmcraft.project');
    assert.equal(filmcraftFile.project.notes.includes('Unrecorded editorial plan'), true);
    const sequence = Object.values(filmcraftFile.project.items).find((item) => item.kind?.Sequence);
    assert.ok(sequence, 'FilmCraft native sequence is absent');
    assert.equal(sequence.kind.Sequence.video_tracks.length, 3);
    assert.equal(sequence.kind.Sequence.markers.length, 4);
    assert.ok(sequence.kind.Sequence.video_tracks.every((track) => track.items.length === 0));
    assert.deepEqual(captureRequest, editPlan.captureRequest);
    assert.equal(editPlan.captureRequest.demoSha256, sourceHash);
    assert.equal(editPlan.captureRequest.eventId, finderTarget.item.id);
    assert.equal(editPlan.captureRequest.throwTick, finderTarget.item.tick);
    assert.equal(editPlan.status, 'planning_only');
    assert.equal(editPlan.version, 2);
    assert.equal(editPlan.editorFrameRate, 60);
    assert.equal(editPlan.lineupHoldMs, 2000);
    assert.equal(editPlan.lineupHoldFrames, 120);
    const pov = editPlan.spans.filter((span) => span.cameraRole === 'player_pov');
    const third = editPlan.spans.filter((span) => span.cameraRole === 'third_person_reference');
    const projectile = editPlan.spans.filter((span) => span.cameraRole === 'projectile');
    assert.deepEqual(pov.map((span) => span.kind), ['play', 'hold', 'play']);
    assert.deepEqual(third.map((span) => span.kind), ['play', 'hold', 'play']);
    assert.equal(projectile.length, 1);
    assert.deepEqual([pov[1].timelineStartMs, pov[1].timelineEndMs],
      [third[1].timelineStartMs, third[1].timelineEndMs]);
    assert.deepEqual([pov[1].timelineStartFrame, pov[1].timelineEndFrame],
      [third[1].timelineStartFrame, third[1].timelineEndFrame]);
    assert.equal(pov[1].timelineEndFrame - pov[1].timelineStartFrame, 120);
    assert.equal(projectile[0].timelineStartFrame, editPlan.throwTimelineFrame);
    assert.equal(pov[2].timelineEndMs, third[2].timelineEndMs);
    assert.equal(projectile[0].timelineStartMs, editPlan.throwTimelineMs);
    assert.equal(editProject.tracks.length, 3);
    assert.equal(editProject.markers[2].time_ms, editPlan.throwTimelineMs);
    assert.deepEqual(editProject.assets, []);
    assert.deepEqual(editProject.clips, []);
    assert.deepEqual(editProject.capture_sources, []);
    // The editor-runtime job opens these exact files produced by the native
    // public-demo UI. Keep this fixture transient and source-bound.
    const editorInputDir = path.join(outputDir, 'offline-handoff');
    fs.mkdirSync(editorInputDir, { recursive: true });
    for (const sourceFile of Object.values(handoffPaths)) {
      fs.copyFileSync(sourceFile, path.join(editorInputDir, path.basename(sourceFile)));
    }
    const mismatch = await page.evaluate(async (item) => {
      const path = localStorage.getItem('csdm-port-selected-library');
      const matchId = Number(location.hash.match(/local-(\d+)/)?.[1]);
      try {
        await window.__TAURI_INTERNALS__.invoke('csdm_export_edit_handoff',
          { path, matchId: matchId + 1, eventId: item.id, lineupHoldMs: 2000 });
        return 'unexpected success';
      } catch (error) { return String(error); }
    }, finderTarget.item);
    assert.match(mismatch, /does not belong to this recorded match/);
    const savedPaths = page.locator('[data-selected-throw] [role="status"] details');
    await savedPaths.locator('summary').click();
    assert.equal(await savedPaths.getAttribute('open'), '');
    assert.ok(await savedPaths.getByText('FilmCraft:', { exact: false }).isVisible());
    await savedPaths.locator('summary').click();
    assert.equal(await savedPaths.getAttribute('open'), null);
    await page.getByText('Offline plan saved locally.').scrollIntoViewIfNeeded();
    const handoffShot = path.join(outputDir, 'real-demo-finder-edit-handoff.png');
    await captureAppOnly(page, handoffShot);
    console.log('PUBLIC_EDIT_HANDOFF', JSON.stringify({
      eventId: editPlan.captureRequest.eventId, sourceHash: editPlan.captureRequest.demoSha256,
      spans: editPlan.spans.length, sharedHoldMs: editPlan.lineupHoldMs,
      projectTracks: editProject.tracks.length, mediaAssets: editProject.assets.length,
    }));
    await page.getByRole('link', { name: 'Open at throw in 2D Viewer' }).click();
    await page.getByRole('heading', { name: `2D Viewer - Round ${finderTarget.item.round}` })
      .waitFor({ timeout: 60000 });
    await page.waitForFunction(() => document.querySelector('[data-radar-selected]')?.getAttribute('data-radar-selected') === 'true',
      null, { timeout: 60000 });
    await page.waitForFunction((tick) => Number(document.querySelector('[data-current-tick]')?.getAttribute('data-current-tick')) === tick,
      finderTarget.item.tick, { timeout: 60000 });
    const finderViewerShot = path.join(outputDir, 'real-demo-finder-viewer-link.png');
    await captureAppOnly(page, finderViewerShot);
    // The pinned CSDM Viewer2D canvas, playback bar, rounds bar and drawing
    // hooks receive bounded native frame chunks through the Tauri adapter.
    await page.getByRole('link', { name: '2D Viewer' }).click();
    await page.getByRole('heading', { name: /2D Viewer - Round 1/ }).waitFor({ timeout: 60000 });
    await page.locator('[data-original-csdm-viewer="true"]').waitFor({ timeout: 60000 });
    await page.getByLabel('Choose local radar PNG').setInputFiles(radarPath);
    await page.locator('[data-original-csdm-viewer="true"] canvas').first().waitFor({ timeout: 15000 });
    await page.waitForFunction(() => {
      const root = document.querySelector('[data-original-csdm-viewer="true"]');
      const canvas = root?.querySelector('canvas');
      if (root?.getAttribute('data-radar-selected') !== 'true' || !canvas || canvas.width === 0) return false;
      const context = canvas.getContext('2d');
      if (!context) return false;
      let brightPixels = 0;
      for (let row = 1; row <= 7; row++) {
        for (let column = 1; column <= 7; column++) {
          const x = Math.floor(canvas.width / 2 + (column - 4) * canvas.height / 10);
          const y = Math.floor(row * canvas.height / 8);
          const [red, green, blue] = context.getImageData(x, y, 1, 1).data;
          if (red + green + blue > 240) brightPixels++;
        }
      }
      return brightPixels >= 5;
    }, null, { timeout: 15000 });
    const viewerFacts = await page.evaluate(async () => {
      const library = localStorage.getItem('csdm-port-selected-library');
      const matchId = Number(location.hash.match(/local-(\d+)/)?.[1]);
      if (!library || !Number.isSafeInteger(matchId)) throw new Error('Local viewer route not available');
      const invoke = window.__TAURI_INTERNALS__.invoke;
      const meta = await invoke('get_viewer_round', { path: library, matchId, roundNumber: 1 });
      if (!meta.positionsAvailable) throw new Error('Native frame stream was not persisted');
      let candidate = null;
      for (let start = meta.startTick; start <= meta.endTick; start += 512) {
        const chunk = await invoke('get_viewer_frames', {
          path: library, matchId, roundNumber: 1,
          startTick: start, endTick: Math.min(start + 511, meta.endTick),
        });
        const grenade = chunk.grenades.find((frame) => chunk.players.some((player) => player.tick === frame.tick));
        if (grenade) { candidate = { tick: grenade.tick, players: chunk.players.filter((player) => player.tick === grenade.tick).length,
          grenades: chunk.grenades.filter((frame) => frame.tick === grenade.tick).length }; break; }
      }
      return { meta, candidate };
    });
    assert.ok(viewerFacts.meta.playerSampleCount > 1000, 'Native player frame stream is unexpectedly sparse');
    assert.ok(viewerFacts.meta.grenadeSampleCount > 0, 'Native projectile frame stream is absent');
    assert.ok(viewerFacts.meta.utilityEvents.some((event) => event.detonateTick !== null),
      'No grounded grenade lifecycle events reached the viewer');
    assert.equal(viewerFacts.meta.bombEventsAvailable, true, 'New native import lost bomb event provenance');
    assert.ok(viewerFacts.candidate, 'No synchronized player and projectile frame was found');
    const eventList = page.locator('details.native-recorded-events');
    await eventList.locator('summary').click({ noWaitAfter: true });
    const damageEvent = eventList.locator('[data-event-kind="damage"]').first();
    await damageEvent.waitFor({ timeout: 15000 });
    const damageTick = Number(await damageEvent.getAttribute('data-event-tick'));
    await damageEvent.click();
    await page.waitForFunction((tick) =>
      Number(document.querySelector('[data-current-tick]')?.getAttribute('data-current-tick')) === tick,
    damageTick, { timeout: 15000 });
    assert.equal(await damageEvent.getAttribute('aria-current'), 'true');
    const eventShot = path.join(outputDir, 'real-demo-viewer-recorded-events.png');
    await captureAppOnly(page, eventShot);
    await eventList.locator('summary').click({ noWaitAfter: true });
    await page.evaluate(() => document.activeElement?.blur());
    const timeline = page.locator('[data-original-csdm-viewer="true"] div.border-x-gray-300').first();
    await timeline.evaluate((element, selected) => {
      const bounds = element.getBoundingClientRect();
      const ratio = (selected.tick - selected.start) / (selected.end - selected.start);
      element.dispatchEvent(new MouseEvent('click', { bubbles: true,
        clientX: bounds.left + bounds.width * ratio, clientY: bounds.top + bounds.height / 2 }));
    }, { tick: viewerFacts.candidate.tick, start: viewerFacts.meta.startTick, end: viewerFacts.meta.endTick });
    await page.waitForFunction((selectedTick) => {
      const root = document.querySelector('[data-current-tick]');
      return Math.abs(Number(root?.getAttribute('data-current-tick')) - selectedTick) < 128
        && Number(root?.getAttribute('data-player-count')) > 0;
    }, viewerFacts.candidate.tick, { timeout: 30000 });
    // The original CSDM timeline click calls play(newTick). Observe that
    // transition before toggling Space so the smoke never samples a pause.
    await page.locator('[data-is-playing="true"]').waitFor({ timeout: 15000 });
    await page.keyboard.press('Space');
    await page.locator('[data-is-playing="false"]').waitFor({ timeout: 15000 });
    const viewerText = await page.locator('[data-current-tick]').innerText();
    assert.ok(!viewerText.includes('\uFFFD'), 'Viewer contains a broken replacement glyph');
    assert.ok(await page.locator('[data-original-csdm-viewer="true"] canvas').count() >= 2,
      'Pinned CSDM map and drawing canvases did not mount');
    assert.ok(Number(await page.locator('[data-player-count]').getAttribute('data-player-count')) >= 5,
      'Recorded players were not available at the selected tick');
    const viewerSeekShot = path.join(outputDir, 'real-demo-viewer-seek.png');
    await captureAppOnly(page, viewerSeekShot);
    await page.keyboard.press('Space');
    await page.locator('[data-is-playing="true"]').waitFor({ timeout: 15000 });
    const playbackSample = () => page.evaluate(() => ({
      tick: Number(document.querySelector('[data-current-tick]')?.getAttribute('data-current-tick')),
      wall: performance.now(),
    }));
    const oneXStart = await playbackSample();
    await sleep(1200);
    const oneXEnd = await playbackSample();
    await page.keyboard.press('Space');
    await page.locator('[data-is-playing="false"]').waitFor({ timeout: 15000 });
    const oneXElapsed = (oneXEnd.wall - oneXStart.wall) / 1000;
    const pausedTick = Number(await page.locator('[data-current-tick]').getAttribute('data-current-tick'));
    const oneXRate = (oneXEnd.tick - oneXStart.tick) / oneXElapsed;
    console.log('VIEWER_1X_TIMING', JSON.stringify({ oneXStart, oneXEnd, oneXRate }));
    assert.ok(oneXRate > 35 && oneXRate < 100, `1x playback rate drifted: ${oneXRate}`);
    await sleep(500);
    assert.equal(Number(await page.locator('[data-current-tick]').getAttribute('data-current-tick')), pausedTick,
      'Pause did not hold the synchronized absolute tick');
    await page.keyboard.press('ArrowRight');
    await page.getByText('1.5x', { exact: true }).waitFor({ timeout: 5000 });
    await page.keyboard.press('Space');
    await page.locator('[data-is-playing="true"]').waitFor({ timeout: 15000 });
    const fastStart = await playbackSample();
    await sleep(1200);
    const fastEnd = await playbackSample();
    await page.keyboard.press('Space');
    await page.locator('[data-is-playing="false"]').waitFor({ timeout: 15000 });
    const fastElapsed = (fastEnd.wall - fastStart.wall) / 1000;
    const fastRate = (fastEnd.tick - fastStart.tick) / fastElapsed;
    const speedRatio = fastRate / oneXRate;
    console.log('VIEWER_PLAYBACK_TIMING', JSON.stringify({ oneXRate, fastRate, speedRatio, fastStart, fastEnd }));
    assert.ok(speedRatio > 1.15 && speedRatio < 1.9,
      `1.5x playback did not advance ticks at the selected speed: ${speedRatio}`);
    await page.keyboard.press('n');
    await page.getByRole('heading', { name: /2D Viewer - Round 2/ }).waitFor({ timeout: 60000 });
    await page.waitForFunction(() => Number(document.querySelector('[data-player-count]')?.getAttribute('data-player-count')) > 0,
      null, { timeout: 30000 });
    const viewerRoundShot = path.join(outputDir, 'real-demo-viewer-next-round.png');
    await captureAppOnly(page, viewerRoundShot);
    // Change rounds while bounded IPC chunks are in flight, then traverse the
    // actual WebView2 history. The old loader must not paint stale round data.
    await page.evaluate(() => {
      location.hash = location.hash.replace(/viewer-2d\/\d+/, 'viewer-2d/3');
    });
    await page.waitForTimeout(50);
    await page.goBack();
    await page.getByRole('heading', { name: '2D Viewer - Round 2' }).waitFor({ timeout: 60000 });
    await page.waitForFunction(() =>
      document.querySelector('[data-round-number]')?.getAttribute('data-round-number') === '2'
      && Number(document.querySelector('[data-player-count]')?.getAttribute('data-player-count')) > 0,
    null, { timeout: 30000 });
    await page.goForward();
    await page.getByRole('heading', { name: '2D Viewer - Round 3' }).waitFor({ timeout: 60000 });
    await page.waitForFunction(() =>
      document.querySelector('[data-round-number]')?.getAttribute('data-round-number') === '3'
      && Number(document.querySelector('[data-player-count]')?.getAttribute('data-player-count')) > 0,
    null, { timeout: 30000 });
    assert.ok(Number(await page.locator('[data-incendiary-trajectory-count]').getAttribute('data-incendiary-trajectory-count')) >= 3,
      'Observed incendiary releases did not label the generic Source 2 fire projectile paths');
    const betweenRounds = page.locator('details.native-recorded-events');
    if (!(await betweenRounds.evaluate((element) => element.open))) {
      await betweenRounds.locator('summary').click({ noWaitAfter: true });
    }
    const outsideReplay = betweenRounds.locator('[data-event-kind="bomb"][data-event-tick="21853"]');
    await outsideReplay.waitFor({ timeout: 15000 });
    assert.ok(await outsideReplay.isDisabled(), 'Inter-round bomb plant must not seek to an invented playable frame');
    assert.ok((await outsideReplay.innerText()).includes('between rounds'));
    const betweenRoundsShot = path.join(outputDir, 'real-demo-viewer-between-rounds.png');
    await captureAppOnly(page, betweenRoundsShot);
    await page.goBack();
    await page.getByRole('heading', { name: '2D Viewer - Round 2' }).waitFor({ timeout: 60000 });
    const bombRound = await page.evaluate(async () => {
      const library = localStorage.getItem('csdm-port-selected-library');
      const matchId = Number(location.hash.match(/local-(\d+)/)?.[1]);
      const invoke = window.__TAURI_INTERNALS__.invoke;
      for (let roundNumber = 1; roundNumber <= 10; roundNumber++) {
        const meta = await invoke('get_viewer_round', { path: library, matchId, roundNumber });
        const plant = meta.bombEvents.find((event) => event.kind === 'bomb_planted'
          && event.x !== null && event.y !== null && event.z !== null
          && event.tick >= meta.startTick && event.tick <= meta.endTick);
        if (plant) return { roundNumber, tick: plant.tick, eventCount: meta.bombEvents.length };
      }
      return null;
    });
    console.log('BOMB_VIEWER_FACT', JSON.stringify(bombRound));
    assert.ok(bombRound, 'Pinned public demo has no grounded bomb plant for viewer acceptance');
    await page.evaluate((number) => {
      location.hash = location.hash.replace(/viewer-2d\/\d+/, `viewer-2d/${number}`);
    }, bombRound.roundNumber);
    await page.getByRole('heading', { name: `2D Viewer - Round ${bombRound.roundNumber}` })
      .waitFor({ timeout: 60000 });
    await page.waitForFunction((tick) =>
      document.querySelector('[data-bomb-plant-tick]')?.getAttribute('data-bomb-plant-tick') === String(tick),
    bombRound.tick, { timeout: 60000 });
    const bombList = page.locator('details.native-recorded-events');
    if (!(await bombList.evaluate((element) => element.open))) {
      await bombList.locator('summary').click({ noWaitAfter: true });
    }
    await bombList.locator(`[data-event-kind="bomb"][data-event-tick="${bombRound.tick}"]`).click();
    await page.waitForFunction((tick) =>
      Number(document.querySelector('[data-current-tick]')?.getAttribute('data-current-tick')) === tick,
    bombRound.tick, { timeout: 15000 });
    const bombShot = path.join(outputDir, 'real-demo-viewer-bomb-event.png');
    await captureAppOnly(page, bombShot);
    // Exercise failure recovery in the same native window. Neither attempt
    // may replace or duplicate the successfully imported match.
    await page.evaluate(() => { location.hash = '#/matches'; });
    await page.getByRole('button', { name: 'Import demo' }).click();
    const importPath = page.getByRole('textbox', { name: 'Import a CS2 demo' });
    await importPath.fill(demoPath);
    await page.getByRole('button', { name: 'Import demo' }).last().click();
    await page.getByRole('alert').filter({ hasText: 'already imported' }).waitFor({ timeout: 30000 });
    const duplicateShot = path.join(outputDir, 'duplicate-demo-error.png');
    await captureAppOnly(page, duplicateShot);
    const corruptPath = path.join(outputDir, 'invalid-demo.dem');
    fs.writeFileSync(corruptPath, 'not a valid CS2 demo');
    await importPath.fill(corruptPath);
    await page.getByRole('button', { name: 'Import demo' }).last().click();
    await page.getByRole('alert').filter({ hasText: 'not a CS2 PBDEMS2 demo' }).waitFor({ timeout: 30000 });
    const invalidShot = path.join(outputDir, 'invalid-demo-error.png');
    await captureAppOnly(page, invalidShot);
    assert.equal(await page.getByRole('row').filter({ hasText: 'public-cs2-fixture.dem' }).count(), 1);
    assert.equal(sha256(demoPath), sourceHash, 'Native import changed public demo bytes');
    assert.deepEqual(pageErrors, [], `Renderer errors: ${pageErrors.join('; ')}`);
    const proof = {
      source: 'actual hosted Tauri WebView2 window using the native source2-demo decoder',
      fixture: 'LaihoE/demoparser@45ca85ae:src/parser/test_demo.dem',
      fixtureBytes: fs.statSync(demoPath).size,
      fixtureSha256: sourceHash,
      commit: process.env.GITHUB_SHA || null,
      executableSha256: sha256(appPath),
      featureFreeReleaseBuild: featureFree,
      syntheticEnvironmentDeliberatelySet: featureFree,
      parserCapability: capability,
      observed: { roundRows, playerRows, playerDetailRows, utilityRows, finderTarget, nativeFacts, viewerFacts, pausedTick, bombRound },
      screenshots: Object.fromEntries([matchesShot, overviewShot, roundsShot, roundDetailShot, selectedRoundShot, effectiveDamageShot, duelsShot, duelEventsShot, playersShot, playerDetailShot, utilityShot, finderShot, handoffShot, finderViewerShot, eventShot, viewerSeekShot, viewerRoundShot, betweenRoundsShot, bombShot, duplicateShot, invalidShot]
        .map((file) => [path.basename(file), sha256(file)])),
      runtimeUrl: page.url(),
      scope: 'real demo import and data-limited match views; full upstream CSDM match contracts remain unported',
    };
    fs.writeFileSync(path.join(outputDir, 'proof.json'), `${JSON.stringify(proof, null, 2)}\n`);
    console.log('PASS: public CS2 demo imported and rendered in native Tauri WebView2');
  } catch (error) {
    if (page) {
      await page.screenshot({ path: path.join(outputDir, 'real-demo-failure.png'), animations: 'disabled' }).catch(() => {});
      const visible = await page.evaluate(() => ({ url: location.href, text: document.body.innerText.slice(0, 3000) }))
        .catch((inspectionError) => ({ inspectionError: String(inspectionError) }));
      fs.writeFileSync(path.join(outputDir, 'failure.json'), `${JSON.stringify({ error: String(error), pageErrors, visible }, null, 2)}\n`);
      console.error('Native app visible state:', JSON.stringify({ visible, pageErrors }));
    }
    throw error;
  } finally {
    if (browser) await browser.close().catch(() => {});
    if (exit === null) app.kill();
  }
}

main().catch((error) => { console.error(error); process.exitCode = 1; });
