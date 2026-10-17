import React, { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router';
import { Status } from 'csdm/common/types/status';
import { MatchesActionBar } from 'csdm/ui/matches/actionbar/action-bar';
import { MatchesTableStatusBar } from 'csdm/ui/matches/table/matches-status-bar';
import { MatchesTableProvider } from 'csdm/ui/matches/matches-table-provider';
import { useFetchMatches } from 'csdm/ui/matches/use-fetch-matches';
import { useMatchesStatus } from 'csdm/ui/matches/use-matches-status';
import { ActionBar } from 'csdm/ui/components/action-bar';
import { Button } from 'csdm/ui/components/buttons/button';
import { CenteredContent } from 'csdm/ui/components/content';
import { Table } from 'csdm/ui/components/table/table';
import { useTable } from 'csdm/ui/components/table/use-table';
import type { Column } from 'csdm/ui/components/table/table-types';
import { MatchesTable } from './matches-table';
import { describe, durationSeconds, invoke, libraryPathKey, matchRouteKey } from './match-data';
import { DemoImport } from './demo-import';
import type { ImportJob } from './demo-import';
import type { LibrarySnapshot, MatchSummary } from './match-data';

const columns: readonly Column<MatchSummary>[] = [
  { id: 'fileName', accessor: 'fileName', headerText: 'Demo', headerTooltip: 'Demo file name', width: 210 },
  { id: 'mapName', accessor: 'mapName', headerText: 'Map', headerTooltip: 'Recorded map', width: 125 },
  { id: 'scoreCt', accessor: 'scoreCt', headerText: 'CT', headerTooltip: 'Recorded CT score', width: 55, textAlign: 'right' },
  { id: 'scoreT', accessor: 'scoreT', headerText: 'T', headerTooltip: 'Recorded T score', width: 55, textAlign: 'right' },
  { id: 'roundCount', accessor: 'roundCount', headerText: 'Rounds', headerTooltip: 'Recorded round rows', width: 75, textAlign: 'right' },
  { id: 'playerCount', accessor: 'playerCount', headerText: 'Players', headerTooltip: 'Recorded player rows', width: 75, textAlign: 'right' },
  { id: 'utilityCount', accessor: 'utilityCount', headerText: 'Utility', headerTooltip: 'Recorded utility events', width: 70, textAlign: 'right' },
  { id: 'duration', accessor: (match) => durationSeconds(match) ?? 'Unavailable',
    headerText: 'Duration', headerTooltip: 'Duration from recorded ticks and tick rate; - means unavailable', width: 105,
    formatter: (seconds: number | string) => typeof seconds === 'number' ? `${Math.floor(seconds / 60)}m ${seconds % 60}s` : '-' },
  { id: 'importedAt', accessor: 'importedAt', headerText: 'Imported', headerTooltip: 'DemoStrike import time', width: 175,
    formatter: (value: string) => {
      const date = new Date(value);
      return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
    } },
];

function SourceMatches({ path, onChangeLibrary, onOpenLibrary }: { path: string; onChangeLibrary: () => void; onOpenLibrary: (path: string) => void }) {
  const navigate = useNavigate();
  const [matches, setMatches] = useState<MatchSummary[]>([]);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [error, setError] = useState('');
  const [refresh, setRefresh] = useState(0);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [search, setSearch] = useState('');
  const [showImport, setShowImport] = useState(false);
  const [jobs, setJobs] = useState<ImportJob[]>([]);
  const [queueError, setQueueError] = useState('');
  const seenCompleted = useRef(new Set<number>());
  const table = useTable<MatchSummary>({
    columns,
    data: matches,
    getRowId: (match) => matchRouteKey(match.id),
    rowSelection: 'single',
    fuzzySearchColumnIds: ['fileName', 'mapName'],
    sortedColumn: { id: 'importedAt', direction: 'desc' },
    onSelectionChanged: (instance) => setSelectedId(instance.getSelectedRows()[0]?.id ?? null),
    onRowDoubleClick: (match) => void navigate(matchRouteKey(match.id)),
    onSelectWithKeyboard: (match) => void navigate(matchRouteKey(match.id)),
  });

  useEffect(() => {
    let active = true;
    setStatus('loading');
    setError('');
    void invoke<LibrarySnapshot>('list_matches', { path }).then((snapshot) => {
      if (!active) return;
      setMatches(snapshot.matches);
      setSelectedId(null);
      localStorage.setItem(libraryPathKey, path);
      setStatus('ready');
    }).catch((failure) => {
      if (!active) return;
      setMatches([]);
      setError(describe(failure));
      setStatus('error');
    });
    return () => { active = false; };
  }, [path, refresh]);

  useEffect(() => {
    let active = true;
    async function pollQueue() {
      try {
        const next = await invoke<ImportJob[]>('csdm_list_import_jobs');
        if (!active) return;
        let newlyCompleted = false;
        for (const job of next) {
          if (job.state === 'completed' && !seenCompleted.current.has(job.id)) {
            seenCompleted.current.add(job.id);
            newlyCompleted = true;
          }
        }
        setJobs(next);
        setQueueError('');
        if (newlyCompleted) setRefresh((value) => value + 1);
      } catch (failure) {
        if (active) setQueueError(describe(failure));
      }
    }
    void pollQueue();
    const timer = window.setInterval(() => void pollQueue(), 1200);
    return () => { active = false; window.clearInterval(timer); };
  }, []);

  return (
    <>
      <ActionBar
        left={<>
          <Button onClick={onChangeLibrary}>Change library</Button>
          <Button onClick={() => setRefresh((value) => value + 1)}>Refresh</Button>
          <Button onClick={() => setShowImport((value) => !value)}>{showImport ? 'Close import' : 'Import demo'}</Button>
          <Button isDisabled={selectedId === null} onClick={() => {
            if (selectedId !== null) void navigate(matchRouteKey(selectedId));
          }}>Open match</Button>
        </>}
        right={<label className="flex items-center gap-x-8">
          <span>Find match</span>
          <input aria-label="Find match" className="rounded-4 border border-gray-400 bg-gray-75 p-8"
            value={search} onChange={(event) => {
              setSearch(event.target.value);
              table.setFuzzySearchText(event.target.value);
            }} />
        </label>}
      />
      {showImport && <div className="border-b border-gray-300 px-16 py-12">
        <div className="max-w-[540px]"><DemoImport onQueued={(job) => {
          setJobs((current) => [job, ...current.filter((item) => item.id !== job.id)]);
        }} /></div>
      </div>}
      {(jobs.length > 0 || queueError) && <section className="border-b border-gray-300 px-16 py-8" aria-label="Import queue">
        <h2 className="text-body-strong">Import queue</h2>
        {queueError && <p role="alert" className="mt-4 text-red-500 select-text">Could not read import jobs: {queueError}</p>}
        {jobs.slice(0, 5).map((job) => <p key={job.id} className="mt-4 break-all text-gray-700" role={job.state === 'failed' ? 'alert' : 'status'}>
          <span className="text-body-strong">{job.state === 'running' ? 'Analyzing' : job.state === 'completed' ? 'Imported' : job.state === 'failed' ? 'Import failed' : 'Queued'}</span>
          {' '}{job.demoPath}{job.recovered ? ' · resumed after restart' : ''}{job.error ? `: ${job.error}` : ''}
        </p>)}
      </section>}
      <div className="border-b border-gray-300 px-16 py-8">
        <span className="text-body-strong">Existing DemoStrike library</span>
        <span className="ml-8 break-all text-gray-700 select-text">{path}</span>
      </div>
      {status === 'loading' && <CenteredContent><p role="status">Loading recorded matches.</p></CenteredContent>}
      {status === 'error' && <CenteredContent><div className="flex max-w-[520px] flex-col items-center gap-y-8 text-center" role="alert">
        <h1 className="text-subtitle">Could not open this library</h1>
        <p className="select-text">{error}</p>
        <Button onClick={() => setRefresh((value) => value + 1)}>Try again</Button>
      </div></CenteredContent>}
      {status === 'ready' && matches.length === 0 && <CenteredContent>
        <p className="text-subtitle">This library has no recorded matches.</p>
      </CenteredContent>}
      {status === 'ready' && matches.length > 0 && <Table table={table} />}
      <div className="flex justify-end border-t border-gray-300 px-16 py-8 text-gray-700">
        {status === 'ready' ? `${matches.length} recorded ${matches.length === 1 ? 'match' : 'matches'} · Read only` : 'Read only'}
      </div>
    </>
  );
}

function OriginalEmptyMatches({ onOpenLibrary }: { onOpenLibrary: (path: string) => void }) {
  const fetchMatches = useFetchMatches();
  const status = useMatchesStatus();
  useEffect(() => { if (status === Status.Idle) void fetchMatches(); });
  return (
    <MatchesTableProvider>
      <MatchesActionBar />
      <MatchesTable onOpenLibrary={onOpenLibrary} />
      <MatchesTableStatusBar />
    </MatchesTableProvider>
  );
}

// The original router, LeftBar, and table engine remain in use. Only fields
// actually recorded by DemoStrike appear in the populated table; CT/T scores
// are not mislabeled as CSDM Team A/B scores or invented combat statistics.
export function Matches() {
  const [path, setPath] = useState(() => localStorage.getItem(libraryPathKey) ?? '');
  if (!path) return <OriginalEmptyMatches onOpenLibrary={setPath} />;
  return <SourceMatches path={path} onOpenLibrary={setPath} onChangeLibrary={() => {
    localStorage.removeItem(libraryPathKey);
    setPath('');
  }} />;
}
