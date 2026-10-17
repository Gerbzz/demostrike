import path from 'node:path';
import { fileURLToPath } from 'node:url';
import upstream from '../upstream/cs-demo-manager/vite.config.ts';

const portedRoot = path.dirname(fileURLToPath(import.meta.url));
const originalSource = path.resolve(portedRoot, '../upstream/cs-demo-manager/src');
const upstreamModules = path.resolve(portedRoot, '../upstream/cs-demo-manager/node_modules');

export default {
  ...upstream,
  root: portedRoot,
  define: {
    ...upstream.define,
    // Electron injected this at runtime; the Tauri renderer must supply it
    // at build time. Keep a single production startup/effect cycle.
    REACT_STRICT_MODE_ENABLED: 'false',
  },
  resolve: {
    ...upstream.resolve,
    alias: [
      // The ported component lives outside the upstream package root, while
      // React Compiler adds this import after resolving its source file.
      { find: /^react$/, replacement: path.resolve(upstreamModules, 'react/index.js') },
      { find: /^react-router$/, replacement: path.resolve(upstreamModules, 'react-router') },
      { find: 'react/compiler-runtime', replacement: path.resolve(upstreamModules, 'react/compiler-runtime.js') },
      { find: 'react/jsx-runtime', replacement: path.resolve(upstreamModules, 'react/jsx-runtime.js') },
      { find: './index.css', replacement: path.resolve(portedRoot, 'index.css') },
      { find: 'csdm/ui/settings/database/database-settings', replacement: path.resolve(portedRoot, 'database-settings.tsx') },
      { find: './database/database-settings', replacement: path.resolve(portedRoot, 'database-settings.tsx') },
      { find: './error-boundary', replacement: path.resolve(portedRoot, 'error-boundary.tsx') },
      { find: 'csdm/ui/matches/table/matches-table', replacement: path.resolve(portedRoot, 'matches-table.tsx') },
      { find: 'csdm/ui/matches/matches', replacement: path.resolve(portedRoot, 'matches.tsx') },
      { find: 'csdm/ui/match/match-loader', replacement: path.resolve(portedRoot, 'match-loader.tsx') },
      { find: 'csdm/ui/match/overview/match-overview', replacement: path.resolve(portedRoot, 'csdm-partial-overview.tsx') },
      { find: 'csdm/ui/match/rounds/overview/rounds', replacement: path.resolve(portedRoot, 'csdm-partial-rounds.tsx') },
      { find: 'csdm/ui/match/rounds/round/round', replacement: path.resolve(portedRoot, 'csdm-partial-rounds.tsx') },
      { find: 'csdm/ui/match/players/match-players-loader', replacement: path.resolve(portedRoot, 'match-players.tsx') },
      { find: 'csdm/ui/match/players/match-players', replacement: path.resolve(portedRoot, 'match-players.tsx') },
      { find: 'csdm/ui/match/grenades/match-grenades', replacement: path.resolve(portedRoot, 'match-utility.tsx') },
      { find: 'csdm/ui/match/grenades/stats/grenades-stats', replacement: path.resolve(portedRoot, 'match-utility.tsx') },
      { find: 'csdm/ui/match/grenades/finder/grenades-finder-loader', replacement: path.resolve(portedRoot, 'native-grenades-finder.tsx') },
      { find: 'csdm/ui/match/heatmap/match-heatmap', replacement: path.resolve(portedRoot, 'unsupported-match-view.tsx') },
      { find: 'csdm/ui/match/duels/match-duels', replacement: path.resolve(originalSource, 'ui/match/duels/match-duels.tsx') },
      { find: 'csdm/ui/match/duels/opening-duels-map/opening-duels-map-loader', replacement: path.resolve(portedRoot, 'unsupported-match-view.tsx') },
      { find: 'csdm/ui/match/duels/opening-duels-stats/opening-duels-stats', replacement: path.resolve(portedRoot, 'unsupported-match-view.tsx') },
      { find: 'csdm/ui/match/duels/players-duels-matrix', replacement: path.resolve(portedRoot, 'observed-duels.tsx') },
      { find: 'csdm/ui/match/weapons/weapons', replacement: path.resolve(portedRoot, 'unsupported-match-view.tsx') },
      // Mount the pinned CSDM canvas, controls, and drawing hooks. Only the
      // Electron data/radar edges and unrecorded equipment/audio panels adapt.
      { find: 'csdm/ui/match/viewer-2d/viewer-2d-loader', replacement: path.resolve(portedRoot, 'native-original-viewer-2d.tsx') },
      { find: 'csdm/ui/match/use-current-match', replacement: path.resolve(portedRoot, 'native-original-viewer-match.ts') },
      { find: '../use-current-match', replacement: path.resolve(portedRoot, 'native-original-viewer-match.ts') },
      { find: '../../use-current-match', replacement: path.resolve(portedRoot, 'native-original-viewer-match.ts') },
      { find: './drawing/use-draw-map-radar', replacement: path.resolve(portedRoot, 'native-original-viewer-radar.tsx') },
      { find: './drawing/use-draw-players', replacement: path.resolve(portedRoot, 'native-original-draw-players.ts') },
      { find: 'csdm/ui/match/viewer-2d/teams-panel/team-panel', replacement: path.resolve(portedRoot, 'native-original-team-panel.tsx') },
      { find: './audio-selector-button', replacement: path.resolve(portedRoot, 'native-viewer-no-audio.tsx') },
      { find: 'csdm/ui/match/video/video-loader', replacement: path.resolve(portedRoot, 'unsupported-match-view.tsx') },
      { find: 'csdm/ui/match/chat-messages/chat-messages', replacement: path.resolve(portedRoot, 'unsupported-match-view.tsx') },
      { find: 'csdm/ui/match/economy/economy', replacement: path.resolve(portedRoot, 'unsupported-match-view.tsx') },
      { find: 'csdm/ui/demos/demos', replacement: path.resolve(portedRoot, 'demos.tsx') },
      { find: 'csdm/ui/web-socket-client', replacement: path.resolve(portedRoot, 'web-socket-client.ts') },
      // The preserved provider imports this client relatively; redirect that
      // edge too so the app never attempts a localhost Electron WebSocket.
      { find: '../web-socket-client', replacement: path.resolve(portedRoot, 'web-socket-client.ts') },
      { find: /^csdm\//, replacement: `${originalSource}/` },
    ],
  },
  build: {
    ...upstream.build,
    outDir: path.resolve(portedRoot, 'dist'),
    emptyOutDir: true,
  },
};
