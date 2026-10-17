import React from 'react';
import { Link, useNavigate, useParams } from 'react-router';
import { Content } from 'csdm/ui/components/content';
import { TeamScore } from 'csdm/ui/components/match/team-score';
import { Table } from 'csdm/ui/components/table/table';
import { useTable } from 'csdm/ui/components/table/use-table';
import type { Column } from 'csdm/ui/components/table/table-types';
import { buildMatchPlayerPath } from 'csdm/ui/routes-paths';
import { durationSeconds } from './match-data';
import type { MatchDetail, MatchSummary, RecordedRound } from './match-data';
import { useSourceMatch } from './match-loader';

// Adapted from pinned CSDM MatchInformation -> Scoreboard -> RoundsTimeline.
// Its full Match requires stable teams and ratings; the native library does
// not provide them, so this variant accepts only the actual recorded fields.
function MatchInformation({ summary }: { summary: MatchSummary }) {
  const duration = durationSeconds(summary);
  const imported = new Date(summary.importedAt);
  return <div className="flex flex-wrap gap-16 border-b border-gray-300 pb-16">
    <div className="flex h-[124px] w-[160px] shrink-0 items-center justify-center border border-gray-300 bg-gray-75 p-12 text-center text-gray-700">
      Map preview unavailable
    </div>
    <div className="flex min-w-[180px] flex-col gap-y-4">
      <h1 className="text-title select-text">{summary.mapName}</h1>
      <p className="text-gray-700 select-text">{summary.fileName}</p>
      <p className="mt-8 text-body-strong">Rounds won by side</p>
      <div className="flex flex-wrap gap-x-20 gap-y-4">
        <TeamScore teamName="CT" teamScore={summary.scoreCt} scoreOppositeTeam={summary.scoreT} />
        <TeamScore teamName="T" teamScore={summary.scoreT} scoreOppositeTeam={summary.scoreCt} />
      </div>
      <p className="text-gray-700">CT and T are round sides, not stable teams.</p>
    </div>
    <dl className="grid min-w-[220px] flex-1 grid-cols-1 content-start gap-x-16 gap-y-4 sm:grid-cols-2">
      <div><dt className="text-gray-700">Imported</dt><dd>{Number.isNaN(imported.getTime()) ? summary.importedAt : imported.toLocaleString()}</dd></div>
      <div><dt className="text-gray-700">Demo duration</dt><dd>{duration === null ? 'Unavailable' : `${Math.floor(duration / 60)}m ${duration % 60}s`}</dd></div>
      <div><dt className="text-gray-700">Tick rate</dt><dd>{summary.tickRate ?? 'Unavailable'}</dd></div>
      <div><dt className="text-gray-700">Utility events</dt><dd>{summary.utilityCount}</dd></div>
      <div className="sm:col-span-2"><dt className="text-gray-700">Source checksum</dt><dd className="break-all select-text">{summary.sourceSha256 ?? 'Unavailable'}</dd></div>
    </dl>
  </div>;
}

type ScoreboardRow = {
  [key: string]: unknown;
  steamid: string; name: string;
  kills: number | null; assists: number | null; deaths: number | null;
  rawDamage: number | null; observedRounds: number | null;
};
const fact = (value: number | null) => value === null ? 'Unavailable' : value;
const columns: readonly Column<ScoreboardRow>[] = [
  { id: 'name', accessor: 'name', headerText: 'Player', headerTooltip: 'Open the recorded player', width: 280, allowHiding: false },
  { id: 'kills', accessor: (row) => fact(row.kills), headerText: 'K', headerTooltip: 'Enemy eliminations', width: 105, textAlign: 'right' },
  { id: 'assists', accessor: (row) => fact(row.assists), headerText: 'A', headerTooltip: 'Enemy assists', width: 105, textAlign: 'right' },
  { id: 'deaths', accessor: (row) => fact(row.deaths), headerText: 'D', headerTooltip: 'Deaths', width: 105, textAlign: 'right' },
  { id: 'rawDamage', accessor: (row) => fact(row.rawDamage), headerText: 'Raw DMG', headerTooltip: 'Raw enemy dmg_health; may include overkill', width: 155, textAlign: 'right' },
  { id: 'observedRounds', accessor: (row) => fact(row.observedRounds), headerText: 'Rounds', headerTooltip: 'Rounds with observed player-side facts', width: 130, textAlign: 'right' },
];

function Scoreboard({ detail }: { detail: MatchDetail }) {
  const navigate = useNavigate();
  const { checksum } = useParams();
  const rows: ScoreboardRow[] = detail.players.map((player) => ({
    steamid: player.steamid, name: player.name,
    kills: detail.fullCombatCoverage ? player.combat?.enemyKills ?? null : null,
    assists: detail.fullCombatCoverage ? player.combat?.enemyAssists ?? null : null,
    deaths: detail.fullCombatCoverage ? player.combat?.deaths ?? null : null,
    rawDamage: detail.fullCombatCoverage ? player.combat?.enemyHealthDamage ?? null : null,
    observedRounds: detail.fullCombatCoverage ? player.combat?.rounds.length ?? null : null,
  }));
  const openPlayer = (player: ScoreboardRow) => {
    if (checksum) void navigate(buildMatchPlayerPath(checksum, encodeURIComponent(player.steamid)));
  };
  const table = useTable<ScoreboardRow>({
    columns, data: rows, getRowId: (row) => row.steamid, rowSelection: 'single',
    sortedColumn: { id: 'name', direction: 'asc' },
    onRowDoubleClick: openPlayer, onSelectWithKeyboard: openPlayer,
  });
  return <section className="mt-20" aria-label="Recorded player scoreboard">
    <div className="flex flex-wrap items-baseline justify-between gap-x-16 gap-y-4">
      <h2 className="text-subtitle">Recorded players</h2>
      <p className="text-gray-700">{detail.players.length} players | Double-click or press Enter to open a player</p>
    </div>
    <p className="mt-4 max-w-[760px] text-gray-700">The original CSDM scoreboard groups stable teams and includes ratings. This import records players and observed combat, without verified stable teams or ratings.</p>
    {rows.length === 0 ? <p className="mt-12" role="status">No players were recorded for this match.</p> :
      <div className="mt-12 h-[400px] border border-gray-300"><Table<ScoreboardRow> table={table} /></div>}
    <p className="mt-8 text-gray-700">Raw DMG sums enemy <code>dmg_health</code> events and may include overkill. It is not effective HP removed. Unavailable values were not recorded by this import.</p>
  </section>;
}

function RoundsTimeline({ rounds }: { rounds: RecordedRound[] }) {
  return <section className="mt-20" aria-label="Round winners by observed side">
    <div className="flex flex-wrap items-baseline justify-between gap-x-16 gap-y-4">
      <h2 className="text-subtitle">Round winners by observed side</h2>
      <Link to="rounds" className="text-blue-600 underline focus-visible:outline">All rounds</Link>
    </div>
    {rounds.length === 0 ? <p className="mt-8" role="status">No rounds were recorded for this match.</p> :
      <div className="mt-12 flex gap-x-4 overflow-x-auto pb-8">
        {rounds.map((round) => {
          const winner = round.winner === 'CT' || round.winner === 'T' ? round.winner : null;
          return <Link key={round.roundNumber} to={`rounds/${round.roundNumber}`}
            className="flex min-w-[32px] flex-1 flex-col gap-y-4 text-center focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-500"
            title={`Round ${round.roundNumber}: ${winner ?? 'winner unavailable'}`}
            aria-label={`Round ${round.roundNumber}, ${winner ? `${winner} win` : 'winner unavailable'}`}>
            <span className="h-[3px] w-full" style={{ backgroundColor: winner === 'CT' ? '#74aef6' : winner === 'T' ? '#efb46a' : '#737373' }} />
            <span className="text-gray-700 tabular-nums">{round.roundNumber}</span>
          </Link>;
        })}
      </div>}
  </section>;
}

export function MatchOverview() {
  const detail = useSourceMatch();
  return <Content>
    <MatchInformation summary={detail.summary} />
    <RoundsTimeline rounds={detail.rounds} />
    <Scoreboard detail={detail} />
    <section className="mt-24 border-t border-gray-300 pt-12 text-gray-700" aria-label="Coverage limits">
      <h2 className="text-body-strong">Not recorded by this import</h2>
      <p className="mt-4 max-w-[760px]">Stable Team A/B identity, ratings, KAST, full economy, match date, server and client details, comments, and playback controls remain unavailable. They are not estimated from CT/T sides or substituted with zeros.</p>
      {detail.summary.parserCapability === 'test-synthetic-combat-v1' && <p className="mt-8">Synthetic CI facts bypassed the CS2 decoder; combat values on this match are fixture data.</p>}
    </section>
  </Content>;
}
