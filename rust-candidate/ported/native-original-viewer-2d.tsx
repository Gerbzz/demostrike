import React, { useEffect, useMemo, useState } from 'react';
import { Link, useNavigate, useParams, useSearchParams } from 'react-router';
import { Game, GrenadeName, TeamNumber, WeaponName } from 'csdm/common/types/counter-strike';
import type { Map } from 'csdm/common/types/map';
import type { Kill } from 'csdm/common/types/kill';
import type { GrenadePosition } from 'csdm/common/types/grenade-position';
import { Viewer2D } from 'csdm/ui/match/viewer-2d/viewer-2d';
import { ViewerContext } from 'csdm/ui/match/viewer-2d/viewer-context';
import { buildPlayerId } from 'csdm/ui/match/viewer-2d/build-player-id';
import { describe, invoke, libraryPathKey, matchIdFromRoute } from './match-data';
import type { MatchDetail, RecordedRound, ViewerFrameChunk, ViewerGrenadeFrame, ViewerPlayerFrame, ViewerRoundMeta } from './match-data';
import { useSourceMatch } from './match-loader';
import { nativeSide, originalRound } from './native-original-viewer-match';
import { NativeRadarProvider } from './native-original-viewer-radar';
import type { NativeObservedPosition } from './native-original-draw-players';
import { observedBombTimeline, observedFireSubtypes, observedGrenadeEvents } from './native-original-viewer-events';
import { NativeOriginalEventList } from './native-original-event-list';
import { rememberSelectedRadar, selectedRadarFile } from './native-selected-radar';
import './native-original-viewer.css';

const calibratedMaps: Record<string, Pick<Map, 'posX' | 'posY' | 'scale' | 'radarSize'>> = {
  de_mirage: { posX: -3230, posY: 1713, scale: 5, radarSize: 1024 },
  de_ancient: { posX: -2953, posY: 2164, scale: 5, radarSize: 1024 },
};
const chunkTicks = 512;

type Frames = { players: ViewerPlayerFrame[]; grenades: ViewerGrenadeFrame[] };
type ViewerValue = NonNullable<React.ContextType<typeof ViewerContext>>;

function grenadeName(kind: string | null, fireSubtype?: 'molotov' | 'incendiary'): GrenadeName | null {
  if (kind?.includes('Smoke')) return GrenadeName.Smoke;
  if (kind?.includes('Flash')) return GrenadeName.Flashbang;
  if (kind?.includes('HEGrenade')) return GrenadeName.HE;
  if (kind?.includes('Incendiary')) return GrenadeName.Incendiary;
  if (kind?.includes('Molotov')) return fireSubtype === 'incendiary' ? GrenadeName.Incendiary
    : fireSubtype === 'molotov' ? GrenadeName.Molotov : null;
  if (kind?.includes('Decoy')) return GrenadeName.Decoy;
  return null;
}

function originalViewerYaw(raw: number | null): number {
  if (raw === null) return Number.NaN;
  // This decoder's nonzero Source 2 eye angle is 180 degrees from the
  // pinned CSDM viewer convention. Both pinned exports keep exact zero as zero.
  if (raw === 0) return 0;
  return ((raw % 360 + 360) % 360) - 180;
}

function playerPosition(frame: ViewerPlayerFrame, checksum: string, roundNumber: number): NativeObservedPosition {
  // The original canvas uses position, side, yaw, health and alive state. Its
  // equipment fields are unavailable in the native stream and are not shown
  // in the adapted team panel. NaN suppresses unobserved health/yaw graphics.
  return {
    id: `${frame.tick}-${frame.steamid}`,
    matchChecksum: checksum,
    roundNumber,
    tick: frame.tick,
    frame: Number.NaN,
    playerSteamId: frame.steamid,
    playerName: frame.name,
    side: nativeSide(frame.side),
    x: frame.x, y: frame.y, z: frame.z,
    yaw: originalViewerYaw(frame.yaw),
    health: frame.health ?? Number.NaN,
    isAlive: frame.isAlive === true,
    observedAlive: frame.isAlive,
    flashDurationRemaining: 0,
    money: Number.NaN,
    armor: Number.NaN,
    hasHelmet: false,
    hasBomb: false,
    hasDefuseKit: false,
    isDucking: false,
    isAirborne: false,
    isScoping: false,
    isDefusing: false,
    isPlanting: false,
    isGrabbingHostage: false,
    activeWeaponName: WeaponName.World,
    equipments: '', grenades: '', pistols: '', smgs: '', rifles: '', heavy: '',
  };
}

function grenadePosition(frame: ViewerGrenadeFrame, checksum: string, roundNumber: number,
  fireSubtype?: 'molotov' | 'incendiary'): GrenadePosition | null {
  const name = grenadeName(frame.grenadeType, fireSubtype);
  if (!name) return null;
  return {
    id: `${frame.trajectoryIndex}-${frame.tick}`,
    matchChecksum: checksum,
    roundNumber,
    tick: frame.tick,
    frame: Number.NaN,
    grenadeId: String(frame.entityId),
    projectileId: String(frame.trajectoryIndex),
    grenadeName: name,
    x: frame.x, y: frame.y, z: frame.z,
    throwerSteamId: frame.throwerSteamid ?? '',
    throwerName: '', throwerTeamName: '', throwerSide: TeamNumber.UNASSIGNED,
    throwerVelocityX: Number.NaN, throwerVelocityY: Number.NaN,
    throwerVelocityZ: Number.NaN, throwerYaw: Number.NaN, throwerPitch: Number.NaN,
  };
}

function observedKills(detail: MatchDetail, recorded: RecordedRound, frames: Frames, checksum: string, start: number, end: number): Kill[] {
  const positionByTick = new Map(frames.players.map((frame) => [`${frame.tick}:${frame.steamid}`, frame]));
  const nameFor = (steamid: string | null) => steamid === null ? WeaponName.World
    : detail.players.find((player) => player.steamid === steamid)?.name ?? steamid;
  return (recorded.deaths ?? []).filter((death) => death.tick >= start && death.tick <= end).map((death) => {
    const killer = death.attackerSteamid ? positionByTick.get(`${death.tick}:${death.attackerSteamid}`) : undefined;
    const victim = positionByTick.get(`${death.tick}:${death.victimSteamid}`);
    // Upstream timeline and kill feed only consume these recorded fields.
    // Optional advanced kill badges are unavailable and remain undisplayed.
    return {
      id: String(death.sourceIndex), matchChecksum: checksum, roundNumber: recorded.roundNumber,
      tick: death.tick, frame: Number.NaN,
      killerSteamId: death.attackerSteamid ?? '', killerName: nameFor(death.attackerSteamid),
      killerSide: nativeSide(death.attackerSide), killerTeamName: '',
      killerX: killer?.x ?? Number.NaN, killerY: killer?.y ?? Number.NaN, killerZ: killer?.z ?? Number.NaN,
      victimSteamId: death.victimSteamid, victimName: nameFor(death.victimSteamid),
      victimSide: nativeSide(death.victimSide), victimTeamName: '',
      victimX: victim?.x ?? Number.NaN, victimY: victim?.y ?? Number.NaN, victimZ: victim?.z ?? Number.NaN,
      assisterSteamId: death.assisterSteamid ?? '', assisterName: death.assisterSteamid ? nameFor(death.assisterSteamid) : '',
      assisterSide: TeamNumber.UNASSIGNED, assisterTeamName: '',
      assisterX: Number.NaN, assisterY: Number.NaN, assisterZ: Number.NaN,
      weaponName: (death.weapon ?? WeaponName.World) as WeaponName,
      isKillerAirborne: false, isKillerBlinded: false, isVictimAirborne: false,
      isVictimBlinded: false, isHeadshot: false, penetratedObjects: 0,
      isAssistedFlash: false, isKillerControllingBot: false,
      isVictimControllingBot: false, isAssisterControllingBot: false,
      isTradeKill: false, isTradeDeath: false, isThroughSmoke: false, isNoScope: false,
    } as Kill;
  });
}

function NativeOriginalViewer({ detail, recorded, meta, frames, radarBitmap, initialTick }: {
  detail: MatchDetail; recorded: RecordedRound; meta: ViewerRoundMeta; frames: Frames;
  radarBitmap: ImageBitmap | null; initialTick: number | null;
}) {
  const { checksum } = useParams();
  const navigate = useNavigate();
  const start = meta.startTick!;
  const end = meta.endTick!;
  const round = useMemo(() => originalRound(detail, recorded, meta), [detail, recorded, meta]);
  const map = useMemo<Map>(() => ({
    id: meta.mapName, name: meta.mapName, game: Game.CS2,
    ...calibratedMaps[meta.mapName], thresholdZ: Number.NEGATIVE_INFINITY,
    radarFilePath: radarBitmap ? 'selected in memory' : undefined, lowerRadarFilePath: undefined,
    thumbnailFilePath: undefined,
  }), [meta.mapName, radarBitmap]);
  const [mode, setMode] = useState<'drawing' | 'playback'>('playback');
  const [drawingTool, setDrawingTool] = useState<ViewerValue['drawingTool']>('pen');
  const [drawingSize, setDrawingSize] = useState(2);
  const [drawingColor, setDrawingColor] = useState('#ff0000');
  const [currentTick, setTick] = useState(initialTick ?? recorded.freezeEndTick ?? start);
  const [isPlaying, setIsPlaying] = useState(false);
  const [speed, setSpeed] = useState(1);
  const [focusedPlayerId, setFocusedPlayerId] = useState<string | undefined>();
  const [lowerRadarOffsetX, setLowerRadarOffsetX] = useState(0);
  const [lowerRadarOffsetY, setLowerRadarOffsetY] = useState(0);
  const [lowerRadarOpacity, setLowerRadarOpacity] = useState(1);
  const playerPositions = useMemo(() => frames.players.map((frame) => playerPosition(frame, checksum!, recorded.roundNumber)), [frames.players, checksum, recorded.roundNumber]);
  const fireSubtypes = useMemo(() => observedFireSubtypes(meta, frames.grenades), [meta, frames.grenades]);
  const grenadePositions = useMemo(() => frames.grenades.map((frame) =>
    grenadePosition(frame, checksum!, recorded.roundNumber, fireSubtypes.get(frame.trajectoryIndex)))
    .filter((row): row is GrenadePosition => row !== null), [frames.grenades, checksum, recorded.roundNumber, fireSubtypes]);
  const lifecycle = useMemo(() => observedGrenadeEvents(meta, frames.grenades, checksum!), [meta, frames.grenades, checksum]);
  const bombs = useMemo(() => observedBombTimeline(meta, checksum!), [meta, checksum]);
  const kills = useMemo(() => observedKills(detail, recorded, frames, checksum!, start, end), [detail, recorded, frames, checksum, start, end]);
  const tickrate = meta.tickRate && meta.tickRate > 0 ? meta.tickRate : 64;
  const setCurrentTick = (tick: number) => setTick(Math.max(start, Math.min(tick, end)));
  const changeRound = (number: number) => {
    if (number >= 1 && number <= detail.rounds.length) void navigate(`/matches/${checksum}/viewer-2d/${number}`);
  };
  const context: ViewerValue = {
    mode, setMode, toggleMode: () => setMode((value) => value === 'playback' ? 'drawing' : 'playback'),
    isDrawingMode: mode === 'drawing',
    tickrate, speed, setSpeed, currentTick, setCurrentTick,
    timeRemaining: (end - currentTick) / tickrate * 1000,
    play: (tick?: number) => { if (tick !== undefined) setCurrentTick(tick); setIsPlaying(true); },
    pause: () => setIsPlaying(false),
    playPause: async () => setIsPlaying((value) => !value), isPlaying,
    changeRound, volume: 1, updateVolume: () => {}, updateAudioOffset: () => {},
    resetAudioOffset: () => {}, loadAudioFile: async () => {}, unloadAudioFile: () => {},
    audioBytes: new Uint8Array(), map, game: Game.CS2,
    kills, round, shots: [], playerPositions,
    hostagesPickUpStart: [], heGrenadesExplode: lifecycle.heGrenadesExplode,
    flashbangsExplode: lifecycle.flashbangsExplode,
    smokesStart: lifecycle.smokesStart, decoysStart: lifecycle.decoysStart,
    hostagesPickedUp: [], hostagePositions: [],
    grenadePositions, infernoPositions: [], chickenPositions: [],
    bombsPlantStart: [], bombsDefuseStart: [],
    bombExploded: bombs.bombExploded, bombPlanted: bombs.bombPlanted,
    bombDefused: bombs.bombDefused, shouldDrawBombs: bombs.bombPlanted !== null,
    focusedPlayerId, updateFocusedPlayerId: (id: string) => setFocusedPlayerId((value) => value === id ? undefined : id),
    lowerRadarOffsetX, setLowerRadarOffsetX, lowerRadarOffsetY, setLowerRadarOffsetY,
    lowerRadarOpacity, setLowerRadarOpacity,
    drawingTool, setDrawingTool, drawingSize, setDrawingSize, drawingColor, setDrawingColor,
  };
  return <div className="original-viewer-bridge relative flex min-h-0 flex-1 flex-col" data-original-csdm-viewer="true"
    data-round-number={recorded.roundNumber}
    data-radar-selected={radarBitmap ? 'true' : 'false'}
    data-is-playing={isPlaying ? 'true' : 'false'}
    data-current-tick={currentTick} data-player-count={frames.players.filter((row) => row.tick === currentTick).length}
    data-unknown-alive-count={frames.players.filter((row) => row.tick === currentTick && row.isAlive === null).length}
    data-grenade-detonation-count={lifecycle.recorded.length}
    data-incendiary-trajectory-count={[...fireSubtypes.values()].filter((type) => type === 'incendiary').length}
    data-bomb-event-count={bombs.observedCount}
    data-bomb-plant-tick={bombs.bombPlanted?.tick ?? ''}
    data-bomb-events-available={meta.bombEventsAvailable ? 'true' : 'false'}
    data-grenade-count={frames.grenades.filter((row) => row.tick === currentTick).length}>
    <ViewerContext.Provider value={context}>
      <NativeRadarProvider bitmap={radarBitmap}><Viewer2D /></NativeRadarProvider>
      <NativeOriginalEventList detail={detail} recorded={recorded} meta={meta} />
    </ViewerContext.Provider>
  </div>;
}

export function Viewer2DLoader() {
  const detail = useSourceMatch();
  const [searchParams] = useSearchParams();
  const { checksum, number } = useParams();
  const matchId = matchIdFromRoute(checksum);
  const path = localStorage.getItem(libraryPathKey);
  const roundNumber = Number(number || 1);
  const recorded = detail.rounds.find((round) => round.roundNumber === roundNumber);
  const requestedTick = Number(searchParams.get('tick'));
  const initialTick = searchParams.has('tick') && Number.isSafeInteger(requestedTick)
    && typeof recorded?.startTick === 'number' && typeof recorded?.endTick === 'number'
    && requestedTick >= recorded!.startTick! && requestedTick <= recorded!.endTick!
    ? requestedTick : null;
  const [meta, setMeta] = useState<ViewerRoundMeta | null>(null);
  const [frames, setFrames] = useState<Frames | null>(null);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState('');
  const [radarError, setRadarError] = useState('');
  const [radarSelection, setRadarSelection] = useState<{ mapName: string; bitmap: ImageBitmap } | null>(null);
  const radarBitmap = radarSelection !== null && meta !== null && radarSelection.mapName === meta.mapName
    ? radarSelection.bitmap : null;

  useEffect(() => {
    if (matchId === null || !path || !recorded || !Number.isSafeInteger(roundNumber)) {
      setError('Choose a recorded match and round first.');
      return;
    }
    let active = true;
    setMeta(null); setFrames(null); setError(''); setProgress(0);
    const load = async () => {
      try {
        const result = await invoke<ViewerRoundMeta>('get_viewer_round', { path, matchId, roundNumber });
        if (!active) return;
        setMeta(result);
        if (!result.positionsAvailable || result.startTick === null || result.endTick === null) return;
        const chunks: ViewerFrameChunk[] = [];
        const count = Math.ceil((result.endTick - result.startTick + 1) / chunkTicks);
        for (let start = result.startTick; start <= result.endTick; start += chunkTicks) {
          const end = Math.min(result.endTick, start + chunkTicks - 1);
          const chunk = await invoke<ViewerFrameChunk>('get_viewer_frames', {
            path, matchId, roundNumber, startTick: start, endTick: end,
          });
          if (!active) return;
          chunks.push(chunk);
          setProgress(Math.round(chunks.length / count * 100));
        }
        if (active) setFrames({ players: chunks.flatMap((chunk) => chunk.players), grenades: chunks.flatMap((chunk) => chunk.grenades) });
      } catch (failure) { if (active) setError(describe(failure)); }
    };
    void load();
    return () => { active = false; };
  }, [path, matchId, roundNumber, recorded]);

  useEffect(() => () => { radarSelection?.bitmap.close(); }, [radarSelection]);
  useEffect(() => {
    const mapName = meta?.mapName;
    if (!mapName || radarSelection) return;
    const file = selectedRadarFile(mapName);
    if (!file) return;
    let active = true;
    void createImageBitmap(file).then((bitmap) => {
      if (active) setRadarSelection({ mapName, bitmap });
      else bitmap.close();
    }).catch(() => { if (active) setRadarError('The selected PNG could not be decoded.'); });
    return () => { active = false; };
  }, [meta?.mapName, radarSelection]);
  useEffect(() => {
    if (radarSelection && meta?.mapName && radarSelection.mapName !== meta.mapName) setRadarSelection(null);
  }, [meta?.mapName, radarSelection]);
  const chooseRadar = async (file: File | undefined) => {
    if (!file) return;
    setRadarError('');
    if (file.type !== 'image/png' || file.size > 8 * 1024 * 1024) {
      setRadarError('Choose a PNG radar smaller than 8 MB.'); return;
    }
    try {
      const bitmap = await createImageBitmap(file);
      const valid = bitmap.width === 1024 && bitmap.height === 1024;
      if (!valid) { bitmap.close(); setRadarError('This calibrated view needs a 1024 x 1024 radar PNG.'); return; }
      rememberSelectedRadar(meta!.mapName, file);
      setRadarSelection({ mapName: meta!.mapName, bitmap });
    } catch { setRadarError('The selected PNG could not be decoded.'); }
  };
  if (error) return <div role="alert" className="p-16">{error}</div>;
  if (!meta) return <div role="status" className="p-16">Loading recorded round positions...</div>;
  if (!meta.positionsAvailable || !calibratedMaps[meta.mapName] || meta.startTick === null || meta.endTick === null || meta.startTick >= meta.endTick)
    return <div role="status" className="flex flex-col gap-8 p-16">
      <h1 className="text-subtitle">2D viewer unavailable for round {roundNumber}</h1>
      <p className="max-w-[640px] text-gray-700">{!calibratedMaps[meta.mapName]
        ? 'This map has no verified CSDM coordinate calibration.'
        : 'This import has no complete native player-position stream. Existing match views remain readable.'}</p>
      <Link className="text-blue-600 underline" to={`/matches/${checksum}/rounds`}>Back to recorded rounds</Link>
    </div>;
  if (!frames || !recorded) return <div role="status" className="p-16">Loading original viewer frames... {progress}%</div>;
  return <div className="flex min-h-0 flex-1 flex-col">
    <header className="flex flex-wrap items-center justify-between gap-8 border-b border-gray-300 px-16 py-8">
      <div><h1 className="text-subtitle">2D Viewer - Round {roundNumber}</h1>
        <p className="text-gray-700">Recorded positions and detonations. Smoke effect area and expiration, fire cells, flash blindness, bomb sites, audio and equipment are unavailable.</p></div>
      <label className="cursor-pointer rounded border border-gray-300 px-8 py-4 text-blue-600">Choose local radar PNG
        <input className="sr-only" type="file" accept="image/png" aria-label="Choose local radar PNG"
          onChange={(event) => void chooseRadar(event.target.files?.[0])} /></label>
    </header>
    {radarError && <p role="alert" className="px-16 py-4 text-red-600">{radarError}</p>}
    <NativeOriginalViewer key={`${roundNumber}-${initialTick ?? 'default'}`} detail={detail} recorded={recorded}
      meta={meta} frames={frames} radarBitmap={radarBitmap} initialTick={initialTick} />
  </div>;
}
