import React, { useEffect, useMemo, useState } from 'react';
import { Link, useParams } from 'react-router';
import { Game } from 'csdm/common/types/counter-strike';
import type { Map } from 'csdm/common/types/map';
import { getScaledCoordinateX } from 'csdm/ui/maps/get-scaled-coordinate-x';
import { getScaledCoordinateY } from 'csdm/ui/maps/get-scaled-coordinate-y';
import { Content } from 'csdm/ui/components/content';
import { describe, invoke, libraryPathKey, matchIdFromRoute } from './match-data';
import type { FinderThrow } from './match-data';
import { useSourceMatch } from './match-loader';
import { rememberSelectedRadar, selectedRadarFile } from './native-selected-radar';

const calibration: Record<string, Pick<Map, 'posX' | 'posY' | 'scale' | 'radarSize'>> = {
  de_mirage: { posX: -3230, posY: 1713, scale: 5, radarSize: 1024 },
  de_ancient: { posX: -2953, posY: 2164, scale: 5, radarSize: 1024 },
};
const labels: Record<string, string> = {
  smoke: 'Smoke', flash: 'Flashbang', he: 'HE grenade',
  molotov: 'Molotov', incendiary: 'Incendiary', decoy: 'Decoy',
};
const colors: Record<string, string> = {
  smoke: '#b8c1cc', flash: '#f4d879', he: '#e8895d',
  molotov: '#ed704d', incendiary: '#f2a557', decoy: '#8da8cf',
};
const pathFor = (event: FinderThrow) => event.trajectory.length >= 2
  ? event.trajectory.filter((point) => point.every(Number.isFinite)) : [];

export function GrenadesFinderLoader() {
  const detail = useSourceMatch();
  const { checksum } = useParams();
  const matchId = matchIdFromRoute(checksum);
  const libraryPath = localStorage.getItem(libraryPathKey);
  const [throws, setThrows] = useState<FinderThrow[]>([]);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [error, setError] = useState('');
  const [search, setSearch] = useState('');
  const [type, setType] = useState('all');
  const [round, setRound] = useState('all');
  const [side, setSide] = useState('all');
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [manualZoom, setManualZoom] = useState<number | null>(null);
  const [radarUrl, setRadarUrl] = useState<string | null>(() => {
    const file = selectedRadarFile(detail.summary.mapName);
    return file ? URL.createObjectURL(file) : null;
  });
  const [radarError, setRadarError] = useState('');
  const [handoffBusy, setHandoffBusy] = useState(false);
  const [lineupHoldMs, setLineupHoldMs] = useState(1200);
  const [handoffError, setHandoffError] = useState('');
  const [handoff, setHandoff] = useState<{ eventId: number; directory: string;
    planPath: string; projectPath: string; requestPath: string; filmcraftProjectPath: string | null;
    throwTimelineMs: number; lineupHoldMs: number } | null>(null);
  const mapInfo = calibration[detail.summary.mapName];

  useEffect(() => {
    if (matchId === null || !libraryPath) {
      setStatus('error'); setError('Choose a recorded match and library first.'); return;
    }
    let active = true;
    setStatus('loading'); setError(''); setThrows([]);
    void invoke<FinderThrow[]>('get_utility_finder', { path: libraryPath, matchId })
      .then((result) => { if (active) { setThrows(result); setStatus('ready'); } })
      .catch((failure) => { if (active) { setError(describe(failure)); setStatus('error'); } });
    return () => { active = false; };
  }, [matchId, libraryPath]);
  useEffect(() => () => { if (radarUrl) URL.revokeObjectURL(radarUrl); }, [radarUrl]);

  const visible = useMemo(() => throws.filter((event) =>
    (type === 'all' || event.utilityType === type)
    && (round === 'all' || event.roundNumber === Number(round))
    && (side === 'all' || event.throwerSide === side)
    && `${event.throwerName} ${event.utilityType} ${event.roundNumber}`
      .toLocaleLowerCase().includes(search.toLocaleLowerCase())), [throws, type, round, side, search]);
  const selected = visible.find((event) => event.id === selectedId) ?? visible[0] ?? null;
  const selectedPath = selected ? pathFor(selected) : [];
  const selectedRound = detail.rounds.find((item) => item.roundNumber === selected?.roundNumber);
  const canSeek = selected !== null && selectedRound?.startTick !== null && selectedRound?.endTick !== null
    && typeof selectedRound?.startTick === 'number' && typeof selectedRound?.endTick === 'number'
    && selected.throwTick >= selectedRound.startTick && selected.throwTick <= selectedRound.endTick;
  const map: Map | null = mapInfo ? {
    id: detail.summary.mapName, name: detail.summary.mapName, game: Game.CS2,
    ...mapInfo, thresholdZ: Number.NEGATIVE_INFINITY,
    radarFilePath: undefined, lowerRadarFilePath: undefined, thumbnailFilePath: undefined,
  } : null;
  const x = (world: number) => getScaledCoordinateX(map!, 1024, world);
  const y = (world: number) => getScaledCoordinateY(map!, 1024, world);
  const pathPixels = map ? selectedPath.map((point) => [x(point[1]), y(point[2])]) : [];
  const extent = pathPixels.length >= 2 ? {
    minX: Math.min(...pathPixels.map((point) => point[0])),
    maxX: Math.max(...pathPixels.map((point) => point[0])),
    minY: Math.min(...pathPixels.map((point) => point[1])),
    maxY: Math.max(...pathPixels.map((point) => point[1])),
  } : null;
  const fitZoom = extent ? Math.max(1, Math.min(3.5, 640 / Math.max(180,
    extent.maxX - extent.minX, extent.maxY - extent.minY))) : 1;
  const zoom = manualZoom ?? fitZoom;
  const viewSize = 1024 / zoom;
  const centerX = zoom === 1 || !extent ? 512 : (extent.minX + extent.maxX) / 2;
  const centerY = zoom === 1 || !extent ? 512 : (extent.minY + extent.maxY) / 2;
  const viewX = Math.max(0, Math.min(1024 - viewSize, centerX - viewSize / 2));
  const viewY = Math.max(0, Math.min(1024 - viewSize, centerY - viewSize / 2));
  const chooseRadar = (file: File | undefined) => {
    if (!file) return;
    setRadarError('');
    if (file.type !== 'image/png' || file.size > 8 * 1024 * 1024) {
      setRadarError('Choose a PNG radar smaller than 8 MB.'); return;
    }
    void createImageBitmap(file).then((bitmap) => {
      const valid = bitmap.width === 1024 && bitmap.height === 1024;
      bitmap.close();
      if (!valid) { setRadarError('This calibrated view needs a 1024 x 1024 radar PNG.'); return; }
      rememberSelectedRadar(detail.summary.mapName, file);
      setRadarUrl(URL.createObjectURL(file));
    }).catch(() => setRadarError('The selected PNG could not be decoded.'));
  };
  const createHandoff = async () => {
    if (!selected || !libraryPath || matchId === null || handoffBusy) return;
    if (!Number.isInteger(lineupHoldMs) || lineupHoldMs < 250 || lineupHoldMs > 5000) {
      setHandoffError('Choose a lineup pause between 250 and 5000 ms.'); return;
    }
    setHandoffBusy(true); setHandoff(null); setHandoffError('');
    try {
      const saved = await invoke<{ eventId: number; directory: string; planPath: string;
        projectPath: string; requestPath: string; filmcraftProjectPath: string | null;
        throwTimelineMs: number; lineupHoldMs: number }>('csdm_export_edit_handoff',
      { path: libraryPath, matchId, eventId: selected.id, lineupHoldMs });
      setHandoff(saved);
    } catch (failure) { setHandoffError(describe(failure)); }
    finally { setHandoffBusy(false); }
  };

  if (status === 'loading') return <Content><p role="status">Loading recorded utility paths...</p></Content>;
  if (status === 'error') return <Content><div role="alert"><h1 className="text-subtitle">Could not open grenade finder</h1>
    <p className="mt-8">{error}</p><Link to=".." className="mt-8 inline-block text-blue-600 underline">Back to utility</Link></div></Content>;
  return <Content>
    <div className="mb-12 flex flex-wrap items-end justify-between gap-12">
      <div><h1 className="text-title">Grenade finder</h1>
        <p className="mt-4 max-w-[680px] text-gray-700">Select a recorded throw to inspect its path, open the 2D Viewer, or save an offline capture and edit plan. Planning does not start the game or recorder.</p></div>
      <p className="text-gray-700" data-finder-count={visible.length}>{visible.length} of {throws.length} throws</p>
    </div>
    {throws.length === 0 ? <div className="rounded-8 border border-gray-300 p-24" role="status">
      No utility throws were recorded in this match. Import a demo with native utility events to use the finder.
    </div> : <div className="flex min-h-[440px] flex-col gap-12 lg:flex-row">
      <div className="flex min-h-0 w-full shrink-0 flex-col gap-8 lg:w-[380px]">
        <label className="text-body-strong">Search player or grenade
          <input aria-label="Search player or grenade" value={search} onChange={(event) => setSearch(event.target.value)}
            className="mt-4 w-full rounded-4 border border-gray-400 bg-gray-75 p-8" /></label>
        <div className="grid grid-cols-3 gap-8">
          <label className="text-gray-700">Grenade<select aria-label="Grenade type" value={type} onChange={(event) => setType(event.target.value)}
            className="mt-4 w-full rounded-4 border border-gray-400 bg-gray-75 p-8">
            <option value="all">All</option>{[...new Set(throws.map((event) => event.utilityType))].sort().map((item) =>
              <option key={item} value={item}>{labels[item] ?? item}</option>)}</select></label>
          <label className="text-gray-700">Round<select aria-label="Round" value={round} onChange={(event) => setRound(event.target.value)}
            className="mt-4 w-full rounded-4 border border-gray-400 bg-gray-75 p-8">
            <option value="all">All</option>{[...new Set(throws.map((event) => event.roundNumber))].sort((a, b) => a - b).map((item) =>
              <option key={item} value={item}>{item}</option>)}</select></label>
          <label className="text-gray-700">Side<select aria-label="Side" value={side} onChange={(event) => setSide(event.target.value)}
            className="mt-4 w-full rounded-4 border border-gray-400 bg-gray-75 p-8">
            <option value="all">All</option><option value="CT">CT</option><option value="T">T</option></select></label>
        </div>
        <div className="min-h-[280px] flex-1 overflow-auto rounded-8 border border-gray-300" role="listbox" aria-label="Recorded grenade throws">
          {visible.length === 0 ? <p className="p-16 text-gray-700">No throws match these filters. Clear a filter or try another player.</p>
            : visible.map((event) => <button key={event.id} type="button" role="option"
              aria-selected={selected?.id === event.id} onClick={() => {
                setSelectedId(event.id); setManualZoom(null); setHandoff(null); setHandoffError('');
              }}
              data-throw-id={event.id}
              className={`flex w-full items-center gap-8 border-b border-gray-300 p-8 text-left hover:bg-gray-100 ${selected?.id === event.id ? 'bg-blue-100' : ''}`}>
              <span className="h-8 w-8 shrink-0 rounded-full" style={{ background: colors[event.utilityType] ?? '#9aa4b2' }} />
              <span className="min-w-0 flex-1"><strong className="block truncate">{event.throwerName}</strong>
                <span className="text-gray-700">{labels[event.utilityType] ?? event.utilityType} · Round {event.roundNumber} · {event.throwerSide || 'Side unknown'}</span></span>
              <span className="tabular-nums text-gray-700">{event.throwTick}</span>
            </button>)}
        </div>
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-8 rounded-8 border border-gray-300 p-12" data-selected-throw={selected?.id ?? ''}>
        {selected && <div className="flex flex-wrap items-center justify-between gap-8">
          <div><h2 className="text-subtitle">{labels[selected.utilityType] ?? selected.utilityType} · Round {selected.roundNumber}</h2>
            <p className="text-gray-700">{selected.throwerName} · tick {selected.throwTick}
              {selected.detonateTick !== null ? ` · detonation ${selected.detonateTick}` : ''}</p></div>
          <div className="flex flex-wrap items-center gap-8">
            {canSeek ? <Link className="rounded-4 bg-blue-600 px-12 py-8 text-white hover:bg-blue-700"
              to={`/matches/${checksum}/viewer-2d/${selected.roundNumber}?tick=${selected.throwTick}`}>
              Open at throw in 2D Viewer</Link> : <span className="text-gray-700">Throw tick is outside the recorded replay.</span>}
            <label className="flex items-center gap-4 text-gray-700">Lineup pause (ms)
              <input aria-label="Lineup pause (ms)" type="number" min="250" max="5000" step="250"
                value={lineupHoldMs} onChange={(event) => {
                  setLineupHoldMs(Number(event.target.value)); setHandoff(null); setHandoffError('');
                }} className="w-[78px] rounded-4 border border-gray-400 bg-gray-75 p-4 tabular-nums" /></label>
            <button type="button" onClick={() => void createHandoff()} disabled={handoffBusy}
              className="rounded-4 border border-gray-500 px-12 py-8 hover:bg-gray-100 disabled:opacity-50">
              {handoffBusy ? 'Verifying source...' : 'Create offline edit plan'}
            </button>
          </div>
        </div>}
        {handoffError && <p role="alert" className="rounded-4 border border-red-400 p-8 text-red-700">{handoffError}</p>}
        {handoff && handoff.eventId === selected?.id && <div role="status"
          className="rounded-4 border border-green-500 p-8 text-gray-800">
          <strong>Offline plan saved locally.</strong> The POV and third-person lanes share a {handoff.lineupHoldMs / 1000}s lineup pause.
          ClipStrike and FilmCraft projects contain editable cues and empty camera tracks. No capture or media was created.
          <details className="mt-4"><summary className="cursor-pointer text-blue-600 underline">Show saved file paths</summary>
            <div className="mt-4 break-all text-small">CSthrowback request: {handoff.requestPath}</div>
            <div className="break-all text-small">Plan: {handoff.planPath}</div>
            <div className="break-all text-small">Project: {handoff.projectPath}</div>
            {handoff.filmcraftProjectPath && <div className="break-all text-small">FilmCraft: {handoff.filmcraftProjectPath}</div>}
          </details>
        </div>}
        {!map ? <div className="flex min-h-[340px] items-center justify-center bg-gray-100 p-16 text-center text-gray-700">
          This map has no verified CSDM coordinate calibration. Throw details remain available in the list.
        </div> : <div className="relative min-h-[340px] flex-1 overflow-hidden bg-[#1c2228]">
          <svg viewBox={`${viewX} ${viewY} ${viewSize} ${viewSize}`} className="absolute size-full" role="img"
            aria-label={selectedPath.length >= 2 ? `Recorded ${selected?.utilityType} trajectory` : 'Radar with no recorded path'}>
            {radarUrl ? <image href={radarUrl} width="1024" height="1024" /> : <>
              <rect width="1024" height="1024" fill="#1c2228" />
              {Array.from({ length: 17 }, (_, index) => <g key={index} stroke="#35414d" strokeWidth="1">
                <path d={`M ${index * 64} 0 V 1024 M 0 ${index * 64} H 1024`} /></g>)}</>}
            {visible.filter((event) => event.id !== selected?.id).map((event) =>
              <circle key={event.id} cx={x(event.throwPosition[0])} cy={y(event.throwPosition[1])}
                r="7" fill={colors[event.utilityType] ?? '#9aa4b2'} opacity=".45" />)}
            {selectedPath.length >= 2 && <><polyline fill="none" stroke="#17202a" strokeWidth="15"
              strokeLinecap="round" strokeLinejoin="round"
              points={pathPixels.map((point) => point.join(',')).join(' ')} />
              <polyline fill="none" stroke={colors[selected!.utilityType] ?? '#fff'} strokeWidth="8"
                strokeLinecap="round" strokeLinejoin="round"
                points={pathPixels.map((point) => point.join(',')).join(' ')} /></>}
            {selected && <><circle cx={x(selected.throwPosition[0])} cy={y(selected.throwPosition[1])}
              r="13" fill="#fff" stroke="#17202a" strokeWidth="4" />
              {selectedPath.length >= 2 && <circle cx={x(selectedPath.at(-1)![1])} cy={y(selectedPath.at(-1)![2])}
                r="13" fill={colors[selected.utilityType] ?? '#fff'} stroke="#17202a" strokeWidth="4" />}</>}
          </svg>
          {selectedPath.length >= 2 && <div className="absolute right-8 top-8 flex gap-4 rounded-4 bg-[#15202add] p-4 text-white" aria-label="Map zoom controls">
            <button type="button" aria-label="Zoom out" className="rounded-4 px-8 py-4 hover:bg-gray-600"
              onClick={() => setManualZoom(Math.max(1, zoom / 1.4))}>−</button>
            <button type="button" aria-label="Fit selected path" className="rounded-4 px-8 py-4 hover:bg-gray-600"
              onClick={() => setManualZoom(null)}>Fit path</button>
            <button type="button" aria-label="Full map" className="rounded-4 px-8 py-4 hover:bg-gray-600"
              onClick={() => setManualZoom(1)}>Full map</button>
            <button type="button" aria-label="Zoom in" className="rounded-4 px-8 py-4 hover:bg-gray-600"
              onClick={() => setManualZoom(Math.min(4, zoom * 1.4))}>+</button>
          </div>}
          {!radarUrl && <p className="absolute bottom-8 left-8 rounded bg-[#15202acc] px-8 py-4 text-white">Coordinate grid · local radar not selected</p>}
        </div>}
        {selected && <div className="flex flex-wrap items-center justify-between gap-8 text-gray-700">
          <span>{selectedPath.length >= 2 ? `${selectedPath.length} measured path points · white = throw · colored = last observed position`
            : 'Path unavailable for this throw; no trajectory was recorded.'}</span>
          {map && <label className="cursor-pointer text-blue-600 underline">Choose local radar PNG
            <input className="sr-only" type="file" accept="image/png" aria-label="Choose local radar PNG"
              onChange={(event) => chooseRadar(event.target.files?.[0])} /></label>}
        </div>}
        {radarError && <p role="alert" className="text-red-600">{radarError}</p>}
      </div>
    </div>}
  </Content>;
}
