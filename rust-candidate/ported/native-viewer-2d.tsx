import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Link, useParams } from 'react-router';
import type { Map } from 'csdm/common/types/map';
import { getScaledCoordinateX } from 'csdm/ui/maps/get-scaled-coordinate-x';
import { getScaledCoordinateY } from 'csdm/ui/maps/get-scaled-coordinate-y';
import { describe, invoke, libraryPathKey, matchIdFromRoute } from './match-data';
import type { ViewerFrameChunk, ViewerGrenadeFrame, ViewerRoundMeta } from './match-data';
import { useSourceMatch } from './match-loader';

// Pinned CSDM 3.20.1 CS2 default-map calibration. No Valve radar art is
// redistributed; a selected local PNG is held only in this WebView session.
const calibratedMaps: Record<string, Pick<Map, 'posX' | 'posY' | 'scale' | 'radarSize'>> = {
  de_mirage: { posX: -3230, posY: 1713, scale: 5, radarSize: 1024 },
  de_ancient: { posX: -2953, posY: 2164, scale: 5, radarSize: 1024 },
};
const chunkTicks = 512;
const sideColor = (side: string | null) => side === 'CT' ? '#65a9e8' : side === 'T' ? '#e7b75f' : '#9ca3af';
const grenadeColor = (kind: string | null) => {
  if (kind?.includes('Smoke')) return '#b9c5d2';
  if (kind?.includes('Flash')) return '#f4e88d';
  if (kind?.includes('Molotov')) return '#ed8b53';
  if (kind?.includes('HEGrenade')) return '#ec646b';
  return '#b8a6ed';
};
const grenadeLabel = (kind: string | null) => {
  if (kind?.includes('Smoke')) return 'Smoke';
  if (kind?.includes('Flash')) return 'Flash';
  if (kind?.includes('Molotov')) return 'Fire';
  if (kind?.includes('HEGrenade')) return 'HE';
  if (kind?.includes('Decoy')) return 'Decoy';
  return 'Projectile';
};
const displayTickTime = (tick: number, start: number, rate: number) => `${Math.max(0, (tick - start) / rate).toFixed(1)}s`;

export function Viewer2DLoader() {
  const match = useSourceMatch();
  const { checksum, number } = useParams();
  const matchId = matchIdFromRoute(checksum);
  const path = localStorage.getItem(libraryPathKey);
  const roundNumber = Number(number || 1);
  const round = match.rounds.find((item) => item.roundNumber === roundNumber);
  const [meta, setMeta] = useState<ViewerRoundMeta | null>(null);
  const [chunk, setChunk] = useState<ViewerFrameChunk | null>(null);
  const [tick, setTick] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState(1);
  const [error, setError] = useState('');
  const [radarUrl, setRadarUrl] = useState<string | null>(null);
  const [radarError, setRadarError] = useState('');
  const [focusedPlayer, setFocusedPlayer] = useState<string | null>(null);
  const chunkRequest = useRef(0);
  const fractionalTicks = useRef(0);
  const scrubbing = useRef(false);

  useEffect(() => {
    if (matchId === null || !path || !Number.isSafeInteger(roundNumber) || roundNumber < 1) {
      setError('Choose a recorded match and round first.');
      return;
    }
    let active = true;
    setPlaying(false);
    setChunk(null);
    setMeta(null);
    setError('');
    void invoke<ViewerRoundMeta>('get_viewer_round', { path, matchId, roundNumber }).then((result) => {
      if (!active) return;
      setMeta(result);
      setTick(round?.freezeEndTick ?? result.startTick ?? 0);
    }).catch((failure) => { if (active) setError(describe(failure)); });
    return () => { active = false; chunkRequest.current++; };
  }, [path, matchId, roundNumber, round?.freezeEndTick]);

  useEffect(() => {
    if (!meta?.positionsAvailable || matchId === null || !path || meta.startTick === null || meta.endTick === null) return;
    if (chunk && chunk.startTick <= tick && tick <= chunk.endTick) return;
    const start = Math.max(meta.startTick, Math.min(tick - 64, meta.endTick - chunkTicks + 1));
    const end = Math.min(meta.endTick, start + chunkTicks - 1);
    const request = ++chunkRequest.current;
    void invoke<ViewerFrameChunk>('get_viewer_frames', {
      path, matchId, roundNumber, startTick: start, endTick: end,
    }).then((result) => {
      if (request === chunkRequest.current) { setChunk(result); setError(''); }
    }).catch((failure) => {
      if (request === chunkRequest.current) { setChunk(null); setPlaying(false); setError(describe(failure)); }
    });
  }, [meta, path, matchId, roundNumber, tick, chunk]);

  useEffect(() => {
    if (!playing || !meta?.positionsAvailable || meta.endTick === null || !meta.tickRate) return;
    let handle = 0;
    let last = performance.now();
    const advance = (now: number) => {
      const elapsed = Math.min(250, now - last);
      last = now;
      if (!scrubbing.current && chunk && chunk.startTick <= tick && tick <= chunk.endTick) {
        fractionalTicks.current += elapsed * meta.tickRate! * speed / 1000;
        const whole = Math.floor(fractionalTicks.current);
        if (whole > 0) {
          fractionalTicks.current -= whole;
          setTick((current) => Math.min(meta.endTick!, current + whole));
        }
      }
      handle = requestAnimationFrame(advance);
    };
    handle = requestAnimationFrame(advance);
    return () => cancelAnimationFrame(handle);
  }, [playing, meta, speed, chunk, tick]);
  useEffect(() => {
    if (meta?.endTick !== null && tick >= (meta?.endTick ?? Infinity)) setPlaying(false);
  }, [meta?.endTick, tick]);
  useEffect(() => () => { if (radarUrl) URL.revokeObjectURL(radarUrl); }, [radarUrl]);

  const playerTicks = useMemo(() => [...new Set(chunk?.players.map((row) => row.tick) ?? [])], [chunk]);
  const observedTick = chunk && chunk.startTick <= tick && tick <= chunk.endTick
    ? (playerTicks.filter((value) => value <= tick).at(-1) ?? null) : null;
  const players = useMemo(() => observedTick === null ? [] :
    (chunk?.players.filter((row) => row.tick === observedTick) ?? []), [chunk, observedTick]);
  const grenades = useMemo(() => chunk && chunk.startTick <= tick && tick <= chunk.endTick
    ? chunk.grenades.filter((row) => row.tick === tick) : [], [chunk, tick]);
  const map = calibratedMaps[meta?.mapName ?? ''] as Map | undefined;
  const worldPoint = (x: number, y: number) => map
    ? { x: getScaledCoordinateX(map, 1024, x), y: getScaledCoordinateY(map, 1024, y) } : null;
  const start = meta?.startTick ?? 0;
  const end = meta?.endTick ?? start;
  const rate = meta?.tickRate ?? 64;
  const deaths = round?.deaths ?? [];
  const throws = match.utilityEvents.filter((event) => event.roundNumber === roundNumber);
  const playerName = (steamid: string | null) =>
    steamid === null ? 'World' : match.players.find((player) => player.steamid === steamid)?.name ?? steamid;
  const recentEvents = [
    ...deaths.map((event) => ({ tick: event.tick,
      label: `${playerName(event.attackerSteamid)} -> ${playerName(event.victimSteamid)}  -  ${event.weapon ?? 'unknown weapon'}` })),
    ...throws.map((event) => ({ tick: event.throwTick, label: `${event.throwerName} threw ${event.utilityType}` })),
  ].filter((event) => event.tick <= tick).sort((a, b) => b.tick - a.tick).slice(0, 5);

  const chooseRadar = async (file: File | undefined) => {
    if (!file) return;
    setRadarError('');
    if (file.type !== 'image/png' || file.size > 8 * 1024 * 1024) {
      setRadarError('Choose a PNG radar smaller than 8 MB.');
      return;
    }
    try {
      const image = await createImageBitmap(file);
      const valid = image.width === 1024 && image.height === 1024;
      image.close();
      if (!valid) { setRadarError('This calibrated view needs a 1024 x 1024 radar PNG.'); return; }
      setRadarUrl(URL.createObjectURL(file));
    } catch {
      setRadarError('The selected PNG could not be decoded.');
    }
  };

  if (error && !meta) return <div role="alert" className="p-16">{error}</div>;
  if (!meta) return <div role="status" className="p-16">Loading recorded round positions...</div>;
  if (!meta.positionsAvailable || !map || start >= end) return <div className="flex flex-col gap-8 p-16" role="status">
    <h1 className="text-title">2D viewer unavailable for round {roundNumber}</h1>
    <p className="max-w-[640px] text-gray-700">{!map
      ? 'This map has no verified CSDM coordinate calibration in the native viewer.'
      : 'This import has no verified per-frame player positions for this round. Older libraries remain readable and can still use their recorded match views.'}</p>
    <Link className="text-blue-600 underline" to={`/matches/${checksum}/rounds`}>Back to recorded rounds</Link>
  </div>;

  return <div className="flex min-h-0 flex-1 flex-col" data-current-tick={tick}
    data-observed-tick={observedTick ?? ''} data-player-count={players.length} data-grenade-count={grenades.length}>
    <header className="flex flex-wrap items-center justify-between gap-8 border-b border-gray-300 px-16 py-8">
      <div><h1 className="text-subtitle">2D viewer  -  Round {roundNumber}</h1>
        <p className="text-gray-700">Recorded positions at absolute demo ticks  -  {meta.playerSampleCount.toLocaleString()} player frames</p></div>
      <label className="cursor-pointer rounded border border-gray-300 px-8 py-4 text-blue-600">
        Choose local radar PNG<input className="sr-only" type="file" accept="image/png"
          aria-label="Choose local radar PNG" onChange={(event) => void chooseRadar(event.target.files?.[0])} />
      </label>
    </header>
    {radarError && <p role="alert" className="px-16 py-4 text-red-600">{radarError}</p>}
    {error && <p role="alert" className="px-16 py-4 text-red-600">{error}</p>}
    <div className="flex min-h-0 flex-1 flex-col gap-12 overflow-auto p-12 lg:flex-row">
      <section aria-label="Recorded map" className="relative mx-auto aspect-square w-full max-w-[600px] min-w-0 shrink-0 overflow-hidden rounded border border-gray-300 bg-gray-100 lg:mx-0 lg:h-full lg:w-auto">
        <svg viewBox="0 0 1024 1024" className="size-full" role="img" aria-label={`Observed players and projectiles on ${meta.mapName}`}>
          {!radarUrl && <><rect width="1024" height="1024" fill="#1c2228" />
            {Array.from({ length: 17 }, (_, index) => <React.Fragment key={index}>
              <line x1={index * 64} y1="0" x2={index * 64} y2="1024" stroke="#36404a" strokeWidth="1" />
              <line x1="0" y1={index * 64} x2="1024" y2={index * 64} stroke="#36404a" strokeWidth="1" />
            </React.Fragment>)}</>}
          {radarUrl && <image href={radarUrl} width="1024" height="1024" />}
          {grenades.map((grenade) => {
            const point = worldPoint(grenade.x, grenade.y); if (!point) return null;
            return <g key={grenade.trajectoryIndex} transform={`translate(${point.x} ${point.y})`}>
              <path d="M0,-10 10,0 0,10 -10,0Z" fill={grenadeColor(grenade.grenadeType)} stroke="#111" strokeWidth="2" />
            </g>;
          })}
          {players.filter((player) => player.isAlive !== false).map((player) => {
            const point = worldPoint(player.x, player.y); if (!point) return null;
            return <g key={player.steamid} transform={`translate(${point.x} ${point.y})`}
              onClick={() => setFocusedPlayer(player.steamid)} className="cursor-pointer">
              <title>{player.name} - {player.side ?? 'unassigned'} - {player.health ?? 'unknown'} HP</title>
              <circle r={focusedPlayer === player.steamid ? 15 : 11} fill={sideColor(player.side)}
                stroke={focusedPlayer === player.steamid ? '#fff' : '#101820'} strokeWidth="3" />
              {player.yaw !== null && <line x1="0" y1="0" x2={Math.cos(player.yaw * Math.PI / 180) * 23}
                y2={-Math.sin(player.yaw * Math.PI / 180) * 23} stroke="white" strokeWidth="3" />}
              {focusedPlayer === player.steamid && <text x="20" y="5" fill="white" fontSize="17"
                stroke="#111" strokeWidth="3" paintOrder="stroke">{player.name}</text>}
            </g>;
          })}
        </svg>
        {!radarUrl && <p className="absolute bottom-8 left-8 right-8 rounded bg-gray-50 p-8 text-center text-gray-700">
          Add a local CSDM or game radar PNG for map artwork. The grid shows observed world positions only.
        </p>}
        {observedTick === null && <p role="status" className="absolute left-8 top-8 rounded bg-gray-50 p-8">Loading this tick...</p>}
      </section>
      <aside className="flex min-w-[250px] flex-1 flex-col gap-12 overflow-auto" aria-label="Synchronized round state">
        <section><h2 className="text-subtitle">At tick {tick.toLocaleString()}</h2>
          <p className="text-gray-700">{displayTickTime(tick, start, rate)} into round  -  {observedTick === null ? 'loading frames' : `player sample ${observedTick}`}</p></section>
        <section><h2 className="text-body-strong">Players</h2>
          <ul className="mt-4 divide-y divide-gray-300 border-y border-gray-300">
            {[...players].sort((a, b) => (a.side ?? '').localeCompare(b.side ?? '') || a.name.localeCompare(b.name)).map((player) =>
              <li key={player.steamid}><button className="flex w-full items-center justify-between gap-8 py-4 text-left"
                onClick={() => setFocusedPlayer(player.steamid)} aria-pressed={focusedPlayer === player.steamid}>
                <span className="flex items-center gap-4"><span aria-hidden="true" className="inline-block size-8 rounded-full"
                  style={{ backgroundColor: sideColor(player.side) }} /> {player.name}</span>
                <span className="text-gray-700">{player.isAlive === false ? 'Out' : player.health === null ? player.side ?? '-' : `${player.health} HP`}</span>
              </button></li>)}
          </ul>
        </section>
        <section><h2 className="text-body-strong">Active projectiles <span className="text-gray-700">({grenades.length})</span></h2>
          <ul className="mt-4 space-y-4">{grenades.map((grenade: ViewerGrenadeFrame) =>
            <li key={grenade.trajectoryIndex}><span aria-hidden="true" className="mr-4 inline-block size-8 rotate-45"
              style={{ backgroundColor: grenadeColor(grenade.grenadeType) }} /> {grenadeLabel(grenade.grenadeType)}
              <span className="ml-8 text-gray-700">#{grenade.entityId}</span></li>)}</ul>
          {grenades.length === 0 && <p className="mt-4 text-gray-700">No projectile at this observed tick.</p>}</section>
        <section><h2 className="text-body-strong">Recorded events</h2>
          <ol className="mt-4 space-y-4">{recentEvents.map((event, index) =>
            <li key={`${event.tick}-${index}`} className="text-gray-700"><span className="tabular-nums">{event.tick}</span>  -  {event.label}</li>)}</ol>
          {recentEvents.length === 0 && <p className="mt-4 text-gray-700">No recorded combat or utility event yet.</p>}</section>
      </aside>
    </div>
    <footer className="border-t border-gray-300 p-12">
      <div className="mb-8 flex flex-wrap items-center gap-8">
        <Link to={`/matches/${checksum}/viewer-2d/${Math.max(1, roundNumber - 1)}`}
          aria-disabled={roundNumber <= 1} className={roundNumber <= 1 ? 'pointer-events-none text-gray-500' : 'text-blue-600 underline'}>Previous round</Link>
        <button type="button" className="min-w-[76px] rounded border border-gray-300 px-8 py-4"
          onClick={() => setPlaying((value) => !value)} aria-label={playing ? 'Pause replay' : 'Play replay'}>
          {playing ? 'Pause' : 'Play'}
        </button>
        <select aria-label="Playback speed" value={speed} onChange={(event) => setSpeed(Number(event.target.value))}
          className="rounded border border-gray-300 bg-gray-50 px-8 py-4">
          <option value="0.5">0.5x</option><option value="1">1x</option><option value="2">2x</option>
        </select>
        <Link to={`/matches/${checksum}/viewer-2d/${roundNumber + 1}`}
          aria-disabled={roundNumber >= match.rounds.length}
          className={roundNumber >= match.rounds.length ? 'pointer-events-none text-gray-500' : 'text-blue-600 underline'}>Next round</Link>
        <span className="ml-auto tabular-nums text-gray-700">{tick.toLocaleString()} / {end.toLocaleString()}</span>
      </div>
      <div className="relative">
        <div className="pointer-events-none absolute inset-x-0 top-1/2 h-4">
          {deaths.map((event) => <span key={event.sourceIndex} className="absolute h-4 w-1 bg-red-500"
            style={{ left: `${(event.tick - start) / (end - start) * 100}%` }} />)}
          {throws.map((event) => <span key={event.id} className="absolute h-4 w-1 bg-blue-500"
            style={{ left: `${(event.throwTick - start) / (end - start) * 100}%` }} />)}
        </div>
        <input type="range" aria-label="Round timeline" className="w-full" min={start} max={end} step={1} value={tick}
          onPointerDown={() => { scrubbing.current = true; }}
          onPointerUp={() => { scrubbing.current = false; fractionalTicks.current = 0; }}
          onChange={(event) => setTick(Number(event.target.value))} />
      </div>
      <p className="text-gray-700">Blue marks utility releases; red marks deaths. Position samples and events share the absolute demo tick.</p>
    </footer>
  </div>;
}
