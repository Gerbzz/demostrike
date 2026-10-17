export type MatchSummary = {
  [key: string]: unknown;
  id: number;
  fileName: string;
  demoFilePath: string | null;
  mapName: string;
  scoreCt: number;
  scoreT: number;
  importedAt: string;
  parserCapability: string;
  sourceSha256: string | null;
  tickRate: number | null;
  durationTicks: number | null;
  playerCount: number;
  roundCount: number;
  utilityCount: number;
};

export type LibrarySnapshot = { path: string; matches: MatchSummary[] };
export type RoundDeath = {
  sourceIndex: number;
  eventOrder: number | null;
  tick: number;
  weapon: string | null;
  attackerSteamid: string | null;
  attackerSide: string | null;
  victimSteamid: string;
  victimSide: string | null;
  assisterSteamid: string | null;
};

export type RoundHurt = {
  sourceIndex: number;
  eventOrder: number | null;
  tick: number;
  weapon: string | null;
  attackerSteamid: string | null;
  attackerSide: string | null;
  victimSteamid: string;
  victimSide: string | null;
  rawDamage: number;
  effectiveDamage: number | null;
};

export type RecordedRound = {
  roundNumber: number;
  winner: string | null;
  startTick: number | null;
  freezeEndTick: number | null;
  endTick: number | null;
  endReason: string | null;
  teamASide: string | null;
  teamBSide: string | null;
  ctEquipValue: number | null;
  tEquipValue: number | null;
  combat: { ctPlayers: number; tPlayers: number; unassignedPlayers: number; enemyKills: number | null } | null;
  deaths: RoundDeath[] | null;
  hurts: RoundHurt[] | null;
};

export type MatchDetail = {
  summary: MatchSummary;
  fullCombatCoverage: boolean;
  rounds: RecordedRound[];
  players: { steamid: string; name: string; combat: {
    stableTeam: string | null;
    enemyKills: number | null; deaths: number; enemyAssists: number | null;
    enemyHealthDamage: number | null; effectiveEnemyHealthDamage: number | null;
    rounds: { roundNumber: number; side: string | null; stableTeam: string | null;
      enemyKills: number | null; deaths: number; enemyAssists: number | null;
      enemyHealthDamage: number | null; effectiveEnemyHealthDamage: number | null }[];
  } | null }[];
  utilityEvents: {
    id: number; roundNumber: number; utilityType: string; throwerName: string;
    throwerTeam: string; throwTick: number; enemiesBlinded: number | null;
    damageEnemy: number | null;
  }[];
};

export type ViewerRoundMeta = {
  roundNumber: number; startTick: number | null; endTick: number | null;
  tickRate: number | null; mapName: string; positionsAvailable: boolean;
  playerSampleCount: number; grenadeSampleCount: number;
  utilityEvents: ViewerUtilityEvent[];
  bombEvents: ViewerBombEvent[];
  bombEventsAvailable: boolean;
};
export type ViewerUtilityEvent = {
  id: number; utilityType: string; throwTick: number;
  detonateTick: number | null;
  detonateX: number | null; detonateY: number | null; detonateZ: number | null;
  projectileEntityId: number | null;
  throwerSteamid: string; throwerName: string; throwerSide: string;
};
export type ViewerBombEvent = {
  sourceIndex: number; kind: string; tick: number;
  actorSteamid: string | null; actorName: string | null;
  siteId: number | null;
  x: number | null; y: number | null; z: number | null;
};
export type ViewerPlayerFrame = {
  tick: number; steamid: string; name: string; side: string | null;
  x: number; y: number; z: number; isAlive: boolean | null;
  health: number | null; yaw: number | null;
};
export type ViewerGrenadeFrame = {
  tick: number; trajectoryIndex: number; entityId: number;
  grenadeType: string | null; throwerSteamid: string | null;
  x: number; y: number; z: number;
};
export type ViewerFrameChunk = {
  startTick: number; endTick: number;
  players: ViewerPlayerFrame[]; grenades: ViewerGrenadeFrame[];
};

export type FinderThrow = {
  id: number; roundNumber: number; utilityType: string;
  throwerSteamid: string; throwerName: string; throwerSide: string;
  throwTick: number; throwPosition: [number, number, number];
  detonateTick: number | null; detonatePosition: [number, number, number] | null;
  trajectory: [number, number, number, number][];
};

export const libraryPathKey = 'csdm-port-selected-library';
export const matchRouteKey = (id: number) => `local-${id}`;
export function matchIdFromRoute(key: string | undefined) {
  if (!key || !/^local-[1-9]\d*$/.test(key)) return null;
  const id = Number(key.slice(6));
  return Number.isSafeInteger(id) ? id : null;
}

export function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const native = (window as any).__TAURI_INTERNALS__ as
    { invoke: <R>(name: string, input?: Record<string, unknown>) => Promise<R> } | undefined;
  return native ? native.invoke<T>(command, args) : Promise.reject(new Error('The Tauri host is unavailable.'));
}

export function describe(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function durationSeconds(summary: MatchSummary) {
  const rate = summary.tickRate;
  const ticks = summary.durationTicks;
  return rate !== null && Number.isFinite(rate) && rate > 0 && ticks !== null && Number.isFinite(ticks) && ticks >= 0
    ? Math.round(ticks / rate) : null;
}
