import type { SmokeStart } from 'csdm/common/types/smoke-start';
import type { DecoyStart } from 'csdm/common/types/decoy-start';
import type { HeGrenadeExplode } from 'csdm/common/types/he-grenade-explode';
import type { FlashbangExplode } from 'csdm/common/types/flashbang-explode';
import type { BombPlanted } from 'csdm/common/types/bomb-planted';
import type { BombExploded } from 'csdm/common/types/bomb-exploded';
import type { BombDefused } from 'csdm/common/types/bomb-defused';
import type { ViewerGrenadeFrame, ViewerRoundMeta, ViewerUtilityEvent } from './match-data';
import { nativeSide } from './native-original-viewer-match';

type Lifecycle = {
  smokesStart: SmokeStart[];
  decoysStart: DecoyStart[];
  heGrenadesExplode: HeGrenadeExplode[];
  flashbangsExplode: FlashbangExplode[];
  recorded: ViewerUtilityEvent[];
};

// Source 2 uses CMolotovProjectile for both fire grenades. The observed
// weapon_fire release retains the actual subtype. Pair it with one projectile
// lifetime by entity, thrower and tick window; an ambiguous link stays unknown.
export function observedFireSubtypes(
  meta: ViewerRoundMeta, frames: ViewerGrenadeFrame[],
): Map<number, 'molotov' | 'incendiary'> {
  const paths = new Map<number, { entity: number; thrower: string | null; start: number; end: number }>();
  for (const frame of frames) {
    if (!frame.grenadeType?.includes('MolotovProjectile')) continue;
    const path = paths.get(frame.trajectoryIndex);
    if (path) {
      path.start = Math.min(path.start, frame.tick);
      path.end = Math.max(path.end, frame.tick);
    } else {
      paths.set(frame.trajectoryIndex, { entity: frame.entityId,
        thrower: frame.throwerSteamid, start: frame.tick, end: frame.tick });
    }
  }
  const assignments = new Map<number, Array<'molotov' | 'incendiary'>>();
  for (const event of meta.utilityEvents) {
    if ((event.utilityType !== 'molotov' && event.utilityType !== 'incendiary')
      || event.projectileEntityId === null) continue;
    const candidates = [...paths].filter(([, path]) =>
      path.entity === event.projectileEntityId
      && path.thrower === event.throwerSteamid
      && path.start >= event.throwTick - 2 && path.start <= event.throwTick + 32
      && (event.detonateTick === null
        || (event.detonateTick >= path.end && event.detonateTick <= path.end + 16)));
    if (candidates.length !== 1) continue;
    const index = candidates[0]![0];
    assignments.set(index, [...(assignments.get(index) ?? []), event.utilityType]);
  }
  return new Map([...assignments].filter(([, types]) => types.length === 1)
    .map(([index, types]) => [index, types[0]!]));
}

// The imported utility table stores observed detonation ticks and positions.
// Match a projectile only when its recorded entity, thrower and tick window
// identify a trajectory. A missing match never creates a synthetic path.
function projectileId(event: ViewerUtilityEvent, frames: ViewerGrenadeFrame[]): string {
  if (event.projectileEntityId === null || event.detonateTick === null) return `unmatched-${event.id}`;
  const candidates = frames.filter((frame) =>
    frame.entityId === event.projectileEntityId
    && frame.tick >= event.throwTick - 2
    && frame.tick <= event.detonateTick! + 2
    && (frame.throwerSteamid === null || frame.throwerSteamid === event.throwerSteamid));
  if (!candidates.length) return `unmatched-${event.id}`;
  candidates.sort((a, b) => Math.abs(a.tick - event.throwTick) - Math.abs(b.tick - event.throwTick));
  const nearest = candidates[0];
  // Entity reuse or overlapping trajectories would make a link ambiguous.
  if (candidates.some((row) => row.trajectoryIndex !== nearest.trajectoryIndex
      && Math.abs(row.tick - event.throwTick) === Math.abs(nearest.tick - event.throwTick))) {
    return `unmatched-${event.id}`;
  }
  return String(nearest.trajectoryIndex);
}

export function observedGrenadeEvents(
  meta: ViewerRoundMeta, frames: ViewerGrenadeFrame[], checksum: string,
): Lifecycle {
  const result: Lifecycle = {
    smokesStart: [], decoysStart: [], heGrenadesExplode: [], flashbangsExplode: [], recorded: [],
  };
  const start = meta.startTick;
  const end = meta.endTick;
  if (start === null || end === null) return result;
  for (const event of meta.utilityEvents) {
    const tick = event.detonateTick;
    if (tick === null || tick < start || tick > end || tick < event.throwTick) continue;
    result.recorded.push(event);
    if (event.detonateX === null || event.detonateY === null || event.detonateZ === null
      || ![event.detonateX, event.detonateY, event.detonateZ].every(Number.isFinite)) continue;
    const base = {
      id: `native-utility-${event.id}`, matchChecksum: checksum,
      roundNumber: meta.roundNumber, tick, frame: Number.NaN,
      grenadeId: String(event.projectileEntityId ?? event.id),
      projectileId: projectileId(event, frames),
      x: event.detonateX, y: event.detonateY, z: event.detonateZ,
      throwerSteamId: event.throwerSteamid, throwerName: event.throwerName,
      throwerTeamName: '', throwerSide: nativeSide(event.throwerSide),
      throwerVelocityX: Number.NaN, throwerVelocityY: Number.NaN,
      throwerVelocityZ: Number.NaN, throwerYaw: Number.NaN,
      throwerPitch: Number.NaN,
    };
    switch (event.utilityType) {
      case 'smoke': result.smokesStart.push(base); break;
      case 'decoy': result.decoysStart.push(base); break;
      case 'he': result.heGrenadesExplode.push(base); break;
      case 'flash': result.flashbangsExplode.push(base); break;
      // Fire has a detonation, but the decoder has no inferno cell stream.
      // Do not draw a made-up burn area from this single position.
    }
  }
  return result;
}

type BombTimeline = {
  bombPlanted: BombPlanted | null;
  bombExploded: BombExploded | null;
  bombDefused: BombDefused | null;
  observedCount: number;
};

// Source 2 site IDs are retained but not mapped to A/B without a verified
// bombsite contract. The original timeline displays '?' rather than a guess.
export function observedBombTimeline(meta: ViewerRoundMeta, checksum: string): BombTimeline {
  const result: BombTimeline = {
    bombPlanted: null, bombExploded: null, bombDefused: null,
    observedCount: meta.bombEvents.length,
  };
  const plants = meta.bombEvents.filter((event) => event.kind === 'bomb_planted');
  if (plants.length !== 1) return result;
  const plant = plants[0];
  if (plant.x === null || plant.y === null || plant.z === null
    || ![plant.x, plant.y, plant.z].every(Number.isFinite)) return result;
  const base = {
    matchChecksum: checksum, roundNumber: meta.roundNumber, frame: Number.NaN,
    site: '?', planterSteamId: plant.actorSteamid ?? '',
    planterName: plant.actorName ?? 'unknown player',
    isPlanterControllingBot: undefined as unknown as boolean,
    x: plant.x, y: plant.y, z: plant.z,
  };
  result.bombPlanted = { ...base, id: `native-bomb-${plant.sourceIndex}`, tick: plant.tick };
  const explosions = meta.bombEvents.filter((event) =>
    event.kind === 'bomb_exploded' && event.tick >= plant.tick);
  if (explosions.length === 1) {
    const explosion = explosions[0];
    result.bombExploded = {
      ...base, id: `native-bomb-${explosion.sourceIndex}`, tick: explosion.tick,
      x: explosion.x ?? plant.x, y: explosion.y ?? plant.y, z: explosion.z ?? plant.z,
    };
  }
  const defusals = meta.bombEvents.filter((event) =>
    event.kind === 'bomb_defused' && event.tick >= plant.tick);
  if (defusals.length === 1) {
    const defusal = defusals[0];
    result.bombDefused = {
      ...base, id: `native-bomb-${defusal.sourceIndex}`, tick: defusal.tick,
      defuserSteamId: defusal.actorSteamid ?? '', defuserName: defusal.actorName ?? 'unknown player',
      isDefuserControllingBot: undefined as unknown as boolean,
      ctAliveCount: Number.NaN, tAliveCount: Number.NaN,
      x: defusal.x ?? plant.x, y: defusal.y ?? plant.y, z: defusal.z ?? plant.z,
    };
  }
  return result;
}
