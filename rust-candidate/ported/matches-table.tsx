import React, { useEffect, useState } from 'react';
import { Status } from 'csdm/common/types/status';
import { Button } from 'csdm/ui/components/buttons/button';
import { CenteredContent } from 'csdm/ui/components/content';
import { Table } from 'csdm/ui/components/table/table';
import { useMatchesStatus } from 'csdm/ui/matches/use-matches-status';
import { useMatchesTable } from 'csdm/ui/matches/table/use-matches-table';
import { useFetchMatches } from 'csdm/ui/matches/use-fetch-matches';
import { useActiveMatchesFilters } from 'csdm/ui/matches/use-active-matches-filters';
import { useFuzzySearchText } from 'csdm/ui/matches/use-fuzzy-search-text';
import { DemoImport } from './demo-import';
import type { ImportJob } from './demo-import';
import { describe, invoke } from './match-data';

// Keep the original action bar, table, status bar, and route. This replaces
// only the messages that would otherwise claim an unported demo import works.
export function MatchesTable({ onOpenLibrary }: { onOpenLibrary: (path: string) => void }) {
  const [path, setPath] = useState('');
  const [pickerError, setPickerError] = useState('');
  const [queued, setQueued] = useState<ImportJob | null>(null);
  const table = useMatchesTable();
  const status = useMatchesStatus();
  const fetchMatches = useFetchMatches();
  const { hasActiveFilter } = useActiveMatchesFilters();
  const search = useFuzzySearchText();

  useEffect(() => {
    let active = true;
    void invoke<ImportJob[]>('csdm_list_import_jobs').then((jobs) => {
      if (active) setQueued(jobs.find((job) => job.state === 'queued' || job.state === 'running') ?? null);
    }).catch((failure) => { if (active) setPickerError(describe(failure)); });
    return () => { active = false; };
  }, []);

  useEffect(() => {
    if (!queued || queued.state === 'completed' || queued.state === 'failed') return;
    let active = true;
    async function poll() {
      try {
        const jobs = await invoke<ImportJob[]>('csdm_list_import_jobs');
        const latest = jobs.find((job) => job.id === queued?.id);
        if (!active || !latest) return;
        setQueued(latest);
        if (latest.state === 'completed' && latest.receipt) onOpenLibrary(latest.receipt.path);
      } catch (failure) {
        if (active) setPickerError(describe(failure));
      }
    }
    void poll();
    const timer = window.setInterval(() => void poll(), 1200);
    return () => { active = false; window.clearInterval(timer); };
  }, [queued?.id, queued?.state, onOpenLibrary]);

  if (status === Status.Idle || status === Status.Loading || !table.isReady()) {
    return <CenteredContent><p className="text-subtitle">Loading matches.</p></CenteredContent>;
  }

  if (status === Status.Error) {
    return (
      <CenteredContent>
        <div className="flex max-w-[480px] flex-col items-center gap-y-12 text-center">
          <h1 className="text-subtitle">Could not load matches</h1>
          <p>The local library could not supply the full match fields required by this screen.</p>
          <Button onClick={() => void fetchMatches()}>Try again</Button>
        </div>
      </CenteredContent>
    );
  }

  if (table.getRowCount() === 0) {
    return (
      <CenteredContent>
        <div className="flex max-w-[520px] flex-col items-center gap-y-12 text-center">
          <h1 className="text-subtitle">{hasActiveFilter || search ? 'No matches found' : 'Your local library is empty'}</h1>
          <p>
            {hasActiveFilter || search
              ? 'Try a different search or filter.'
              : 'Import a CS2 demo for round and player combat analysis, or open an existing DemoStrike library.'}
          </p>
          {!hasActiveFilter && !search && (
            <div className="flex w-full flex-col gap-y-16 text-left">
              <DemoImport onQueued={setQueued} />
              {queued && <p role={queued.state === 'failed' ? 'alert' : 'status'} className="break-all text-gray-700">
                {queued.state === 'failed' ? `Import failed: ${queued.error}` : queued.state === 'running'
                  ? queued.recovered ? 'Resuming demo analysis after restart. Large matches may take several minutes.'
                    : 'Analyzing demo in the import queue. Large matches may take several minutes.'
                  : queued.recovered ? 'Demo queued again after restart.' : 'Demo queued for import.'}
              </p>}
              <div className="flex w-full flex-col gap-y-8 border-t border-gray-300 pt-16">
              <label htmlFor="local-library-path" className="text-body-strong">Open an existing DemoStrike SQLite library</label>
              <p className="text-gray-700">Paste the path to a library file to inspect its matches, rounds, players, and utility in read-only mode.</p>
              <input id="local-library-path" className="w-full rounded-4 border border-gray-400 bg-gray-75 p-8 select-text"
                value={path} onChange={(event) => setPath(event.target.value)} placeholder="C:\\path\\to\\matches.sqlite" />
              {pickerError && <p role="alert" className="text-red-500 select-text">{pickerError}</p>}
              <div className="flex gap-x-8">
                <Button isDisabled={!path.trim()} onClick={() => onOpenLibrary(path.trim())}>Open local library</Button>
                <Button onClick={() => {
                  setPickerError('');
                  const native = (window as any).__TAURI_INTERNALS__;
                  if (!native?.invoke) { setPickerError('The Tauri host is unavailable.'); return; }
                  void native.invoke('csdm_choose_library').then((selected: string | null) => {
                    if (selected) { setPath(selected); onOpenLibrary(selected); }
                  }).catch((error: unknown) => setPickerError(String(error)));
                }}>Browse.</Button>
              </div>
              </div>
            </div>
          )}
        </div>
      </CenteredContent>
    );
  }

  return <Table table={table} />;
}
