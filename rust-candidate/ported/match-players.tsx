import React, { useMemo } from 'react';
import { Link, useParams } from 'react-router';
import { Content } from 'csdm/ui/components/content';
import { Table } from 'csdm/ui/components/table/table';
import { useTable } from 'csdm/ui/components/table/use-table';
import type { Column } from 'csdm/ui/components/table/table-types';
import { useSourceMatch } from './match-loader';
import type { MatchDetail } from './match-data';

type RecordedPlayer = MatchDetail['players'][number];
type PlayerRow = Record<string, unknown> & {
  steamid: string; name: string; kills: number | string; assists: number | string;
  deaths: number | string; damage: number | string; team: string;
};

// Match the original CSDM scoreboard's sortable K/A/D/DMG table contract.
// Only observed facts appear here; ratings and unverified source fields stay unavailable.
const columns: Column<PlayerRow>[] = [
  { id: 'name', accessor: 'name', headerText: 'Player', headerTooltip: 'Recorded player', width: 220,
    allowHiding: false, Cell: ({ data }) => <Link className="text-blue-600 underline" to={encodeURIComponent(data.steamid)}>{data.name}</Link> },
  { id: 'team', accessor: 'team', headerText: 'Team', headerTooltip: 'Observed Team A/B assignment', width: 76 },
  { id: 'steamid', accessor: 'steamid', headerText: 'Steam ID', headerTooltip: 'Recorded Steam ID', width: 180 },
  { id: 'kills', accessor: 'kills', headerText: 'K', headerTooltip: 'Observed enemy kills', width: 78, textAlign: 'right' },
  { id: 'assists', accessor: 'assists', headerText: 'A', headerTooltip: 'Observed enemy assists', width: 78, textAlign: 'right' },
  { id: 'deaths', accessor: 'deaths', headerText: 'D', headerTooltip: 'In-match deaths', width: 78, textAlign: 'right' },
  { id: 'damage', accessor: 'damage', headerText: 'DMG', headerTooltip: 'Effective enemy health damage', width: 90, textAlign: 'right' },
];

function playerRow(player: RecordedPlayer): PlayerRow {
  return { steamid: player.steamid, name: player.name, team: player.combat?.stableTeam ?? '—',
    kills: player.combat?.enemyKills ?? '-', assists: player.combat?.enemyAssists ?? '-',
    deaths: player.combat?.deaths ?? '-', damage: player.combat?.effectiveEnemyHealthDamage ?? '-' };
}

export function MatchPlayers() {
  const { players, fullCombatCoverage } = useSourceMatch();
  const { steamId } = useParams();
  const selected = steamId === undefined ? null : players.find((player) => player.steamid === steamId);
  const rows = useMemo(() => players.map(playerRow), [players]);
  const table = useTable<PlayerRow>({ columns, data: rows,
    getRowId: (player) => player.steamid, rowSelection: 'none',
    sortedColumn: { id: 'kills', direction: 'desc' } });
  if (steamId !== undefined && !selected) return <Content><p role="alert">This player is not recorded in the selected match.</p></Content>;
  return <Content>
    <h1 className="text-title">{selected ? selected.name : 'Recorded players'}</h1>
    <p className="mt-4 text-gray-700">{fullCombatCoverage
      ? 'DMG is effective enemy HP removed. Raw damage retains the demo value, including overkill. Team A/B follows observed match start and side switches when available.'
      : 'Player identities are recorded. Individual combat statistics are unavailable in this library.'}</p>
    {players.length === 0 ? <p className="mt-16">No players were recorded for this match.</p> : !selected ?
      <div className="mt-16 h-[420px] min-w-0 overflow-hidden rounded-8 border border-gray-300" aria-label="Recorded player scoreboard">
        <Table table={table} />
      </div> :
      <div className="mt-16 shrink-0 overflow-auto rounded-8 border border-gray-300">
        <table className="w-full text-left"><thead className="bg-gray-100"><tr>
          <th className="p-8">Player</th><th className="p-8">Team</th><th className="p-8">Steam ID</th>
          <th className="p-8 text-right">Enemy kills</th><th className="p-8 text-right">Assists</th>
          <th className="p-8 text-right">Deaths</th><th className="p-8 text-right">Effective DMG</th><th className="p-8 text-right">Raw DMG</th>
        </tr></thead><tbody>{(selected ? [selected] : players).map((player) => <tr key={player.steamid} className="border-t border-gray-300">
          <td className="p-8">{selected ? player.name : <Link className="text-blue-600 underline" to={encodeURIComponent(player.steamid)}>{player.name}</Link>}</td>
          <td className="p-8">{player.combat?.stableTeam ?? '—'}</td>
          <td className="p-8 select-text">{player.steamid}</td>
          <td className="p-8 text-right tabular-nums">{player.combat?.enemyKills ?? '-'}</td>
          <td className="p-8 text-right tabular-nums">{player.combat?.enemyAssists ?? '-'}</td>
          <td className="p-8 text-right tabular-nums">{player.combat?.deaths ?? '-'}</td>
          <td className="p-8 text-right tabular-nums">{player.combat?.effectiveEnemyHealthDamage ?? '—'}</td>
          <td className="p-8 text-right tabular-nums">{player.combat?.enemyHealthDamage ?? '—'}</td>
        </tr>)}</tbody></table>
      </div>}
    {selected?.combat && <section className="mt-24 shrink-0" aria-label="Player rounds">
      <h2 className="text-subtitle">Round by round</h2>
      <div className="mt-12 overflow-auto rounded-8 border border-gray-300">
        <table className="w-full text-left"><thead className="bg-gray-100"><tr>
          <th className="p-8">Round</th><th className="p-8">Team</th><th className="p-8">Observed side</th>
          <th className="p-8 text-right">Enemy kills</th><th className="p-8 text-right">Assists</th>
          <th className="p-8 text-right">Deaths</th><th className="p-8 text-right">Effective DMG</th><th className="p-8 text-right">Raw DMG</th>
        </tr></thead><tbody>{selected.combat.rounds.map((round) => <tr key={round.roundNumber} className="border-t border-gray-300">
          <td className="p-8">{round.roundNumber}</td><td className="p-8">{round.stableTeam ?? '—'}</td><td className="p-8">{round.side ?? 'Unavailable'}</td>
          <td className="p-8 text-right tabular-nums">{round.enemyKills ?? '-'}</td>
          <td className="p-8 text-right tabular-nums">{round.enemyAssists ?? '-'}</td>
          <td className="p-8 text-right tabular-nums">{round.deaths}</td>
          <td className="p-8 text-right tabular-nums">{round.effectiveEnemyHealthDamage ?? '—'}</td>
          <td className="p-8 text-right tabular-nums">{round.enemyHealthDamage ?? '—'}</td>
        </tr>)}</tbody></table>
      </div>
    </section>}
    {selected && <Link to=".." relative="path" className="mt-16 shrink-0 text-blue-600 underline">All players</Link>}
  </Content>;
}

export const MatchPlayersLoader = MatchPlayers;
