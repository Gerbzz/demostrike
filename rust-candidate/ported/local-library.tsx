import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Button } from 'csdm/ui/components/buttons/button';

type MatchSummary = {
  id: number; fileName: string; mapName: string; scoreCt: number; scoreT: number;
  importedAt: string; parserCapability: string; playerCount: number;
  roundCount: number; utilityCount: number;
};
type Snapshot = { path: string; matches: MatchSummary[] };
type MatchDetail = {
  summary: MatchSummary;
  rounds: { roundNumber: number; winner: string | null; startTick: number | null; endTick: number | null }[];
  players: { steamid: string; name: string }[];
  utilityEvents: {
    id: number; roundNumber: number; utilityType: string; throwerName: string;
    throwerTeam: string; throwTick: number; enemiesBlinded: number | null; damageEnemy: number | null;
  }[];
};
type Tab = 'overview' | 'rounds' | 'players' | 'utility';

function invoke<T>(name: string, args: Record<string, unknown>): Promise<T> {
  const native = (window as any).__TAURI_INTERNALS__;
  if (!native?.invoke) return Promise.reject(new Error('The Tauri host is unavailable.'));
  return native.invoke(name, args);
}

function describe(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function asDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

export function LocalLibrary({ path, onBack }: { path: string; onBack: () => void }) {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [loadState, setLoadState] = useState<'loading' | 'ready' | 'error'>('loading');
  const [loadError, setLoadError] = useState('');
  const [reload, setReload] = useState(0);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [detail, setDetail] = useState<MatchDetail | null>(null);
  const [detailError, setDetailError] = useState('');
  const [tab, setTab] = useState<Tab>('overview');
  const [search, setSearch] = useState('');
  const detailRequest = useRef(0);

  useEffect(() => {
    let active = true;
    setLoadState('loading');
    setLoadError('');
    void invoke<Snapshot>('list_matches', { path }).then((result) => {
      if (!active) return;
      setSnapshot(result);
      setSelectedId(result.matches[0]?.id ?? null);
      setLoadState('ready');
    }).catch((error) => {
      if (!active) return;
      setSnapshot(null);
      setLoadError(describe(error));
      setLoadState('error');
    });
    return () => { active = false; };
  }, [path, reload]);

  useEffect(() => {
    const serial = ++detailRequest.current;
    setDetail(null);
    setDetailError('');
    setTab('overview');
    if (selectedId === null) return;
    void invoke<MatchDetail>('get_match_detail', { path, matchId: selectedId }).then((result) => {
      if (serial === detailRequest.current) setDetail(result);
    }).catch((error) => {
      if (serial === detailRequest.current) setDetailError(describe(error));
    });
  }, [path, selectedId]);

  const matches = useMemo(() => snapshot?.matches.filter((match) =>
    `${match.fileName} ${match.mapName}`.toLocaleLowerCase().includes(search.toLocaleLowerCase())) ?? [],
    [snapshot, search]);

  return (
    <div className="flex flex-1 flex-col gap-y-16 overflow-y-auto bg-gray-50 p-16">
      <div className="flex flex-wrap items-center justify-between gap-8">
        <div>
          <h1 className="text-title">Existing local library</h1>
          <p className="mt-4 text-gray-700 select-text">{path}</p>
        </div>
        <Button onClick={onBack}>Back to Matches</Button>
      </div>
      {loadState === 'loading' && <p role="status">Opening the library…</p>}
      {loadState === 'error' && (
        <div className="flex flex-col gap-y-8 rounded-8 border border-red-500 p-16" role="alert">
          <h2 className="text-subtitle">Could not open this library</h2>
          <p className="select-text">{loadError}</p>
          <div><Button onClick={() => setReload((value) => value + 1)}>Try again</Button></div>
        </div>
      )}
      {loadState === 'ready' && snapshot && (
        <>
          <div className="flex flex-wrap items-center justify-between gap-8">
            <p>{snapshot.matches.length} {snapshot.matches.length === 1 ? 'match' : 'matches'} · Read only</p>
            <label className="flex items-center gap-x-8">
              <span>Find match</span>
              <input aria-label="Find match" className="rounded-4 border border-gray-400 bg-gray-75 p-8"
                value={search} onChange={(event) => setSearch(event.target.value)} />
            </label>
          </div>
          {snapshot.matches.length === 0 ? (
            <p>This library has no imported matches yet.</p>
          ) : (
            <div className="grid min-h-0 flex-1 grid-cols-1 gap-16 xl:grid-cols-[minmax(320px,2fr)_minmax(360px,3fr)]">
              <div className="overflow-auto rounded-8 border border-gray-300">
                <table className="w-full text-left">
                  <thead className="bg-gray-100"><tr>
                    <th className="p-8">Demo</th><th className="p-8">Map</th>
                    <th className="p-8">CT : T</th><th className="p-8">Rounds</th>
                  </tr></thead>
                  <tbody>{matches.map((match) => (
                    <tr key={match.id} className={selectedId === match.id ? 'bg-gray-200' : 'border-t border-gray-300'}>
                      <td className="p-8"><button type="button" className="text-left text-blue-600 underline"
                        aria-current={selectedId === match.id ? 'true' : undefined}
                        onClick={() => setSelectedId(match.id)}>{match.fileName}</button></td>
                      <td className="p-8">{match.mapName}</td>
                      <td className="p-8 whitespace-nowrap">{match.scoreCt} : {match.scoreT}</td>
                      <td className="p-8">{match.roundCount}</td>
                    </tr>
                  ))}</tbody>
                </table>
                {matches.length === 0 && <p className="p-12">No matches found. Try another search.</p>}
              </div>
              <section className="min-h-[280px] overflow-auto rounded-8 border border-gray-300 p-16" aria-label="Match details">
                {detailError && <p role="alert">Could not load match details: {detailError}</p>}
                {!detail && !detailError && <p role="status">Loading match details…</p>}
                {detail && (
                  <>
                    <div className="mb-12 flex flex-wrap items-baseline justify-between gap-8">
                      <h2 className="text-subtitle">{detail.summary.mapName}</h2>
                      <span>CT {detail.summary.scoreCt} : {detail.summary.scoreT} T</span>
                    </div>
                    <div className="mb-16 flex flex-wrap gap-8" role="tablist" aria-label="Match details">
                      {(['overview', 'rounds', 'players', 'utility'] as const).map((item) => (
                        <button key={item} type="button" role="tab" aria-selected={tab === item}
                          className={tab === item ? 'rounded-4 bg-blue-600 px-12 py-8 text-white' : 'rounded-4 border border-gray-400 px-12 py-8'}
                          onClick={() => setTab(item)}>{item[0].toUpperCase() + item.slice(1)}</button>
                      ))}
                    </div>
                    <div role="tabpanel" className="overflow-auto">
                      {tab === 'overview' && <dl className="grid grid-cols-2 gap-12">
                        <div><dt className="text-gray-700">Demo</dt><dd className="select-text">{detail.summary.fileName}</dd></div>
                        <div><dt className="text-gray-700">Imported</dt><dd>{asDate(detail.summary.importedAt)}</dd></div>
                        <div><dt className="text-gray-700">Players</dt><dd>{detail.summary.playerCount}</dd></div>
                        <div><dt className="text-gray-700">Utility events</dt><dd>{detail.summary.utilityCount}</dd></div>
                        <div><dt className="text-gray-700">Parser</dt><dd>{detail.summary.parserCapability}</dd></div>
                      </dl>}
                      {tab === 'rounds' && <table className="w-full text-left"><thead><tr>
                        <th className="p-8">Round</th><th className="p-8">Winner</th>
                        <th className="p-8">Start tick</th><th className="p-8">End tick</th>
                      </tr></thead><tbody>{detail.rounds.map((round) => <tr key={round.roundNumber} className="border-t border-gray-300">
                        <td className="p-8">{round.roundNumber}</td><td className="p-8">{round.winner ?? 'Unknown'}</td>
                        <td className="p-8">{round.startTick ?? '—'}</td><td className="p-8">{round.endTick ?? '—'}</td>
                      </tr>)}</tbody></table>}
                      {tab === 'players' && <table className="w-full text-left"><thead><tr>
                        <th className="p-8">Player</th><th className="p-8">Steam ID</th>
                      </tr></thead><tbody>{detail.players.map((player) => <tr key={player.steamid} className="border-t border-gray-300">
                        <td className="p-8">{player.name}</td><td className="p-8 select-text">{player.steamid}</td>
                      </tr>)}</tbody></table>}
                      {tab === 'utility' && <table className="w-full text-left"><thead><tr>
                        <th className="p-8">Round</th><th className="p-8">Type</th>
                        <th className="p-8">Thrower</th><th className="p-8">Tick</th><th className="p-8">Blind / damage</th>
                      </tr></thead><tbody>{detail.utilityEvents.map((event) => <tr key={event.id} className="border-t border-gray-300">
                        <td className="p-8">{event.roundNumber}</td><td className="p-8">{event.utilityType}</td>
                        <td className="p-8">{event.throwerName}</td><td className="p-8">{event.throwTick}</td>
                        <td className="p-8">{event.enemiesBlinded ?? 'Unavailable'} / {event.damageEnemy ?? 'Unavailable'}</td>
                      </tr>)}</tbody></table>}
                    </div>
                  </>
                )}
              </section>
            </div>
          )}
        </>
      )}
    </div>
  );
}
