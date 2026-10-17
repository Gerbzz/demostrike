import React, { useState } from 'react';
import { Button } from 'csdm/ui/components/buttons/button';
import { describe, invoke } from './match-data';

export type ImportReceipt = { path: string; matchId: number; mapName: string; playerCount: number; roundCount: number };
export type ImportJob = { id: number; demoPath: string; state: 'queued' | 'running' | 'completed' | 'failed';
  recovered: boolean; receipt: ImportReceipt | null; error: string | null; createdAt: string };

export function DemoImport({ onQueued }: { onQueued: (job: ImportJob) => void }) {
  const [demoPath, setDemoPath] = useState('');
  const [working, setWorking] = useState(false);
  const [error, setError] = useState('');
  async function importDemo() {
    setWorking(true);
    setError('');
    try {
      const job = await invoke<ImportJob>('csdm_enqueue_demo', { demoPath: demoPath.trim() });
      onQueued(job);
    } catch (failure) {
      setError(describe(failure));
    } finally {
      setWorking(false);
    }
  }
  async function chooseDemo() {
    setError('');
    try {
      const selected = await invoke<string | null>('csdm_choose_demo');
      if (selected) setDemoPath(selected);
    } catch (failure) {
      setError(describe(failure));
    }
  }
  return <div className="flex w-full flex-col gap-y-8 text-left">
    <label htmlFor="demo-import-path" className="text-body-strong">Import a CS2 demo</label>
    <p className="text-gray-700">Analyze a local .dem into DemoStrike's managed library. The source file stays in place.</p>
    <input id="demo-import-path" className="w-full rounded-4 border border-gray-400 bg-gray-75 p-8 select-text"
      value={demoPath} onChange={(event) => setDemoPath(event.target.value)}
      disabled={working} placeholder="C:\\path\\to\\match.dem" />
    <div className="flex flex-wrap gap-x-8 gap-y-8">
      <Button isDisabled={working || !demoPath.trim()} onClick={() => void importDemo()}>Import demo</Button>
      <Button isDisabled={working} onClick={() => void chooseDemo()}>Browse demos</Button>
    </div>
    {working && <p role="status" className="text-gray-700">Adding demo to the import queue.</p>}
    {error && <p role="alert" className="text-red-500 select-text">Import failed: {error}</p>}
  </div>;
}
