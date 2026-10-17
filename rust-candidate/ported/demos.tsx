import React, { useEffect, useMemo, useState } from 'react';
import { Button } from 'csdm/ui/components/buttons/button';
import { CenteredContent } from 'csdm/ui/components/content';

type DemoFile = { name: string; path: string; sizeBytes: number; modifiedAt: number | null };
type Native = { invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T> };
const folderKey = 'csdm-port-demo-folder';

function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const native = (window as any).__TAURI_INTERNALS__ as Native | undefined;
  return native ? native.invoke<T>(command, args) : Promise.reject(new Error('The Tauri host is unavailable.'));
}

function describe(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function formatSize(bytes: number) {
  return bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KB` : `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// The preserved router and LeftBar open this route. This narrow read-only
// adapter avoids presenting the upstream analysis, delete, or watch actions
// before their backend contracts exist.
export function Demos() {
  const [folder, setFolder] = useState(() => localStorage.getItem(folderKey) ?? '');
  const [folderInput, setFolderInput] = useState(folder);
  const [files, setFiles] = useState<DemoFile[]>([]);
  const [status, setStatus] = useState<'idle' | 'loading' | 'ready' | 'error'>(folder ? 'loading' : 'idle');
  const [error, setError] = useState('');
  const [pickerError, setPickerError] = useState('');
  const [search, setSearch] = useState('');
  const [refresh, setRefresh] = useState(0);

  useEffect(() => {
    if (!folder) return;
    let active = true;
    setStatus('loading');
    setError('');
    void invoke<DemoFile[]>('csdm_list_demo_files', { folder }).then((result) => {
      if (!active) return;
      setFiles(result);
      setStatus('ready');
    }).catch((failure) => {
      if (!active) return;
      setFiles([]);
      setError(describe(failure));
      setStatus('error');
    });
    return () => { active = false; };
  }, [folder, refresh]);

  const visible = useMemo(() => files.filter((file) =>
    file.name.toLocaleLowerCase().includes(search.toLocaleLowerCase())), [files, search]);

  function openFolder(path: string) {
    const next = path.trim();
    if (!next) return;
    localStorage.setItem(folderKey, next);
    setFolderInput(next);
    setFolder(next);
    setRefresh((value) => value + 1);
  }

  return (
    <div className="flex flex-1 flex-col overflow-y-auto bg-gray-50">
      <div className="flex flex-wrap items-center justify-between gap-8 border-b border-gray-300 p-16">
        <div>
          <h1 className="text-title">Demo files</h1>
          <p className="mt-4 text-gray-700">Inspect local Counter-Strike demos. Analysis and playback are still being ported.</p>
        </div>
        {folder && <Button onClick={() => setRefresh((value) => value + 1)}>Refresh</Button>}
      </div>
      <div className="flex flex-wrap items-end gap-8 border-b border-gray-300 p-16">
        <label className="flex min-w-[320px] flex-1 flex-col gap-y-4" htmlFor="demo-folder-path">
          <span>Demo folder</span>
          <input id="demo-folder-path" className="rounded-4 border border-gray-400 bg-gray-75 p-8 select-text"
            value={folderInput} onChange={(event) => setFolderInput(event.target.value)}
            onKeyDown={(event) => { if (event.key === 'Enter') openFolder(folderInput); }}
            placeholder="C:\\path\\to\\demos" />
        </label>
        <Button isDisabled={!folderInput.trim()} onClick={() => openFolder(folderInput)}>Open folder</Button>
        <Button onClick={() => {
          setPickerError('');
          void invoke<string | null>('csdm_choose_demo_folder').then((selected) => {
            if (selected) openFolder(selected);
          }).catch((failure) => setPickerError(describe(failure)));
        }}>Browse folder</Button>
      </div>
      {pickerError && <p role="alert" className="px-16 pt-8 text-red-500 select-text">{pickerError}</p>}
      {status === 'idle' && (
        <CenteredContent><p className="text-subtitle">Choose a folder to view its demo files.</p></CenteredContent>
      )}
      {status === 'loading' && <CenteredContent><p role="status">Reading demo folder.</p></CenteredContent>}
      {status === 'error' && (
        <CenteredContent><div className="flex max-w-[520px] flex-col items-center gap-y-8 text-center" role="alert">
          <h2 className="text-subtitle">Could not read demo folder</h2>
          <p className="select-text">{error}</p>
          <Button onClick={() => setRefresh((value) => value + 1)}>Try again</Button>
        </div></CenteredContent>
      )}
      {status === 'ready' && (
        <div className="flex flex-1 flex-col gap-y-12 p-16">
          <div className="flex flex-wrap items-center justify-between gap-8">
            <p>{files.length} {files.length === 1 ? 'demo file' : 'demo files'} · Read only · Current folder</p>
            <label className="flex items-center gap-x-8">
              <span>Find demo</span>
              <input aria-label="Find demo" className="rounded-4 border border-gray-400 bg-gray-75 p-8"
                value={search} onChange={(event) => setSearch(event.target.value)} />
            </label>
          </div>
          {files.length === 0 ? (
            <CenteredContent><p className="text-subtitle">No .dem files found in this folder.</p></CenteredContent>
          ) : (
            <div className="overflow-auto rounded-8 border border-gray-300">
              <table className="w-full text-left">
                <thead className="bg-gray-100"><tr>
                  <th className="p-8">Demo</th><th className="p-8">Size</th><th className="p-8">Modified</th>
                </tr></thead>
                <tbody>{visible.map((file) => <tr key={file.path} className="border-t border-gray-300">
                  <td className="p-8 select-text">{file.name}</td>
                  <td className="p-8 whitespace-nowrap">{formatSize(file.sizeBytes)}</td>
                  <td className="p-8 whitespace-nowrap">{file.modifiedAt === null ? 'Unavailable' : new Date(file.modifiedAt * 1000).toLocaleString()}</td>
                </tr>)}</tbody>
              </table>
              {visible.length === 0 && <p className="p-12">No demos found. Try another search.</p>}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
