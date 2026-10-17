import assert from 'node:assert/strict';
import { mergeSettings } from '../ported/settings-merge.mjs';

const current = {
  ui: { theme: 'dark', locale: 'en' },
  matches: { demoSources: ['old'], ranking: 'all' },
  video: { outputFolderPath: 'existing' },
};
const next = mergeSettings(current, { matches: { demoSources: ['new'] } }, true);
assert.deepEqual(next, {
  ui: { theme: 'dark', locale: 'en' },
  matches: { demoSources: ['new'], ranking: 'all' },
  video: { outputFolderPath: 'existing' },
});
assert.deepEqual(current.matches.demoSources, ['old'], 'Input settings were mutated');
assert.deepEqual(mergeSettings(current, { matches: { demoSources: ['new'] } }).matches.demoSources, ['old', 'new']);
console.log('PASS: upstream settings merge preserves the profile and replaces filter arrays when requested');
