import React, { createContext, useContext, useEffect, useState } from 'react';
import { Link, Outlet, useParams } from 'react-router';
import { CenteredContent } from 'csdm/ui/components/content';
import { TabLinks } from 'csdm/ui/components/tabs/tab-links';
import { TabLink } from 'csdm/ui/components/tabs/tab-link';
import { describe, invoke, libraryPathKey, matchIdFromRoute } from './match-data';
import type { MatchDetail } from './match-data';

const MatchContext = createContext<MatchDetail | null>(null);

export function useSourceMatch() {
  const detail = useContext(MatchContext);
  if (!detail) throw new Error('A local match detail is required for this route.');
  return detail;
}

// Preserve the original nested match route and tab components while loading
// only the data actually recorded in the selected DemoStrike library.
export function MatchLoader() {
  const { checksum } = useParams();
  const matchId = matchIdFromRoute(checksum);
  const path = localStorage.getItem(libraryPathKey);
  const [detail, setDetail] = useState<MatchDetail | null>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [error, setError] = useState('');

  useEffect(() => {
    if (matchId === null || !path) {
      setStatus('error');
      setError(matchId === null ? 'This match link is not a supported local-library match.' : 'Choose a library on Matches first.');
      return;
    }
    let active = true;
    setDetail(null);
    setStatus('loading');
    void invoke<MatchDetail>('get_match_detail', { path, matchId }).then((result) => {
      if (!active) return;
      setDetail(result);
      setStatus('ready');
    }).catch((failure) => {
      if (!active) return;
      setError(describe(failure));
      setStatus('error');
    });
    return () => { active = false; };
  }, [path, matchId]);

  if (status === 'loading') return <CenteredContent><p role="status">Loading recorded match.</p></CenteredContent>;
  if (status === 'error' || !detail) return <CenteredContent><div className="flex flex-col items-center gap-y-8 text-center" role="alert">
    <h1 className="text-subtitle">Could not open match</h1>
    <p>{error}</p>
    <Link to="/matches" className="text-blue-600 underline">Back to Matches</Link>
  </div></CenteredContent>;

  return <MatchContext.Provider value={detail}>
    <div className="flex flex-wrap items-center justify-between gap-8 border-b border-gray-300 px-16 py-8">
      <div><Link to="/matches" className="text-blue-600 underline">Matches</Link><span className="mx-8">/</span>
        <span className="text-body-strong">{detail.summary.mapName}</span>
        <span className="ml-8 text-gray-700">Recorded DemoStrike data</span></div>
      <span className="text-gray-700">Read only</span>
    </div>
    {detail.summary.parserCapability === 'test-synthetic-combat-v1' && <p role="note" className="border-b border-gray-300 bg-gray-100 px-16 py-8 text-gray-700">
      Synthetic CI fixture: combat facts bypassed the CS2 demo decoder. These rows test import and display only.
    </p>}
    <TabLinks>
      <TabLink url="">Overview</TabLink>
      <TabLink url="rounds" end={false}>Rounds</TabLink>
      <TabLink url="duels" end={false}>Duels</TabLink>
      <TabLink url="players" end={false}>Players</TabLink>
      <TabLink url="grenades" end={false}>Utility</TabLink>
      <TabLink url="viewer-2d" end={false}>2D Viewer</TabLink>
    </TabLinks>
    <Outlet />
  </MatchContext.Provider>;
}
