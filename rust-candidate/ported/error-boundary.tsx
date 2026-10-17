import React from 'react';
import { useRouteError } from 'react-router';
import { Button } from 'csdm/ui/components/buttons/button';

// The Electron boundary offers log-file, restart and upstream-issue actions
// that are not valid in this host. Keep the error and a working recovery path.
export function ErrorBoundary() {
  const error = useRouteError();
  const details = error instanceof Error ? error.stack ?? error.message : String(error);

  return (
    <div className="flex size-full items-center justify-center bg-gray-50 p-24" role="alert">
      <div className="flex w-full max-w-[640px] flex-col gap-y-16 rounded-8 border border-gray-300 bg-gray-75 p-24">
        <div className="flex flex-col gap-y-4">
          <h1 className="text-title">Could not open this view</h1>
          <p>The workspace hit an error. Reload it to try again; the details below can help diagnose the problem.</p>
        </div>
        <div><Button onClick={() => window.location.reload()}>Reload workspace</Button></div>
        <pre className="max-h-[320px] overflow-auto rounded-4 bg-gray-100 p-12 text-body select-text">{details}</pre>
      </div>
    </div>
  );
}
