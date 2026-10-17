import React from 'react';
import { Content } from 'csdm/ui/components/content';
import { durationSeconds } from './match-data';
import { useSourceMatch } from './match-loader';

export function MatchOverview() {
  const { summary, players, fullCombatCoverage } = useSourceMatch();
  const duration = durationSeconds(summary);
  const imported = new Date(summary.importedAt);
  const total = (field: 'enemyKills' | 'enemyAssists' | 'enemyHealthDamage' | 'deaths') => {
    if (!fullCombatCoverage || players.length === 0) return null;
    return players.reduce<number | null>((sum, player) => {
      const value = player.combat?.[field];
      return sum === null || value === null || value === undefined ? null : sum + value;
    }, 0);
  };
  const fact = (value: number | null) => value === null ? 'Unavailable' : value;
  return <Content>
    <div className="flex flex-wrap items-start justify-between gap-16 border-b border-gray-300 pb-16">
      <div>
        <h1 className="text-title">{summary.mapName}</h1>
        <p className="mt-4 text-gray-700 select-text">{summary.fileName}</p>
      </div>
      <div className="flex gap-x-16 text-subtitle">
        <div><span className="text-gray-700">CT</span> {summary.scoreCt}</div>
        <div><span className="text-gray-700">T</span> {summary.scoreT}</div>
      </div>
    </div>
    <div className="mt-16 grid grid-cols-1 gap-16 sm:grid-cols-2 lg:grid-cols-3">
      <div><h2 className="text-body-strong">Recorded rounds</h2><p>{summary.roundCount}</p></div>
      <div><h2 className="text-body-strong">Recorded players</h2><p>{summary.playerCount}</p></div>
      <div><h2 className="text-body-strong">Utility events</h2><p>{summary.utilityCount}</p></div>
      <div><h2 className="text-body-strong">Tick rate</h2><p>{summary.tickRate ?? 'Unavailable'}</p></div>
      <div><h2 className="text-body-strong">Duration</h2><p>{duration === null ? 'Unavailable' : `${Math.floor(duration / 60)}m ${duration % 60}s`}</p></div>
      <div><h2 className="text-body-strong">Imported</h2><p>{Number.isNaN(imported.getTime()) ? summary.importedAt : imported.toLocaleString()}</p></div>
    </div>
    <section className="mt-24 border-t border-gray-300 pt-16" aria-label="Recorded combat">
      <h2 className="text-subtitle">Recorded combat</h2>
      <p className="mt-4 text-gray-700">{fullCombatCoverage
        ? summary.parserCapability === 'test-synthetic-combat-v1'
          ? 'These public synthetic combat facts test import and display. They did not pass through the CS2 demo decoder.'
          : 'Enemy eliminations, assists, deaths, and raw dmg_health come from non-warmup native events. Raw damage can include overkill; unknown side attribution remains unavailable.'
        : 'This older import does not contain a complete player combat stream. New native imports record it; existing annotations stay with this match.'}</p>
      <dl className="mt-12 grid grid-cols-2 gap-12 sm:grid-cols-4">
        <div><dt className="text-gray-700">Enemy eliminations</dt><dd className="text-body-strong">{fact(total('enemyKills'))}</dd></div>
        <div><dt className="text-gray-700">Enemy assists</dt><dd className="text-body-strong">{fact(total('enemyAssists'))}</dd></div>
        <div><dt className="text-gray-700">Deaths</dt><dd className="text-body-strong">{fact(total('deaths'))}</dd></div>
        <div><dt className="text-gray-700">Raw enemy damage</dt><dd className="text-body-strong">{fact(total('enemyHealthDamage'))}</dd></div>
      </dl>
    </section>
    <div className="mt-24 rounded-8 border border-gray-300 p-16">
      <h2 className="text-subtitle">Source and coverage</h2>
      <p className="mt-8 text-gray-700">{fullCombatCoverage
        ? 'The observed CT/T side can change between rounds. Stable Team A/B identities, ratings, and economy are unavailable; this view does not estimate them.'
        : 'This overview shows fields recorded by the DemoStrike import. Team identities, kills, assists, ratings, and economy are unavailable in this library.'}</p>
      <dl className="mt-12 grid grid-cols-1 gap-8 sm:grid-cols-2">
        <div><dt className="text-body-strong">Parser</dt><dd>{summary.parserCapability}</dd></div>
        <div><dt className="text-body-strong">Source checksum</dt><dd className="break-all select-text">{summary.sourceSha256 ?? 'Unavailable'}</dd></div>
        <div className="sm:col-span-2"><dt className="text-body-strong">Demo path</dt><dd className="break-all select-text">{summary.demoFilePath ?? 'Unavailable'}</dd></div>
      </dl>
    </div>
  </Content>;
}
