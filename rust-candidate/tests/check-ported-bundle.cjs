const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const assets = path.resolve(__dirname, '../ported/dist/assets');
const scripts = fs.readdirSync(assets)
  .filter((name) => name.endsWith('.js'))
  .map((name) => fs.readFileSync(path.join(assets, name), 'utf8'));
assert.ok(scripts.length > 0, 'No compiled renderer JavaScript exists');
const bundle = scripts.join('\n');
const styles = fs.readdirSync(assets)
  .filter((name) => name.endsWith('.css'))
  .map((name) => fs.readFileSync(path.join(assets, name), 'utf8'))
  .join('\n');
assert.ok(bundle.includes('csdm_request'), 'Tauri request adapter was not included');
assert.ok(!bundle.includes('REACT_STRICT_MODE_ENABLED'), 'Electron-only startup constant was left unresolved');
assert.ok(bundle.includes('Existing CS Demo Manager PostgreSQL data is untouched.'), 'Accurate local-library settings pane was not included');
assert.ok(bundle.includes('Could not open this view'), 'Ported recoverable error boundary was not included');
assert.ok(bundle.includes('list_matches') && bundle.includes('get_match_detail'), 'Read-only DemoStrike library bridge was not included');
assert.ok(bundle.includes('Recorded rounds') && bundle.includes('Recorded players'), 'Nested match routes were not included');
assert.ok(bundle.includes('Round history') && bundle.includes('Recorded death events'), 'Partial original rounds layout was not included');
assert.ok(bundle.includes('Raw DMG') && bundle.includes('Map preview unavailable'), 'Partial original overview was not included');
assert.ok(bundle.includes('Recorded utility') && bundle.includes('This match analysis is still being ported'), 'Utility and unsupported-route states were not included');
assert.ok(bundle.includes('csdm_choose_library'), 'Native library picker was not included');
assert.ok(bundle.includes('csdm_enqueue_demo') && bundle.includes('csdm_list_import_jobs') && bundle.includes('csdm_choose_demo'), 'Native demo import queue was not included');
assert.ok(bundle.includes('csdm_list_demo_files') && bundle.includes('csdm_choose_demo_folder'), 'Read-only demo-folder route was not included');
assert.ok(bundle.includes('get_viewer_round') && bundle.includes('get_viewer_frames')
  && bundle.includes('Choose local radar PNG'), 'Native absolute-tick 2D viewer was not included');
assert.ok(!bundle.includes('WS:: connecting to server'), 'Electron WebSocket client remained in the ported bundle');
assert.ok(styles.includes('.gap-x-8'), 'Original UI spacing classes were not generated');
console.log('PASS: ported original renderer contains the Tauri adapter and no Electron WebSocket startup');
