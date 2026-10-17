import React from 'react';
import { Link, useParams } from 'react-router';
import { Content } from 'csdm/ui/components/content';
import { useSourceMatch } from './match-loader';

export function Rounds() {
  const { rounds } = useSourceMatch();
  const { number } = useParams();
  const selected = number === undefined ? null : rounds.find((round) => String(round.roundNumber) === number);
  if (number !== undefined && !selected) return <Content><p role="alert">Round {number} is not recorded in this library.</p></Content>;
  return <Content>
    <h1 className="text-title">{selected ? `Round ${selected.roundNumber}` : 'Recorded rounds'}</h1>
    <p className="mt-4 text-gray-700">Winners and tick boundaries come from the DemoStrike import. Full combat rows also record observed CT/T players and enemy eliminations for each round.</p>
    {rounds.length === 0 ? <p className="mt-16">No rounds were recorded for this match.</p> :
      <div className="mt-16 overflow-auto rounded-8 border border-gray-300">
        <table className="w-full text-left"><thead className="bg-gray-100"><tr>
          <th className="p-8">Round</th><th className="p-8">Winner</th>
          <th className="p-8">Start tick</th><th className="p-8">End tick</th>
          <th className="p-8">Observed CT / T</th><th className="p-8 text-right">Enemy kills</th>
        </tr></thead><tbody>{(selected ? [selected] : rounds).map((round) => <tr key={round.roundNumber} className="border-t border-gray-300">
          <td className="p-8">{selected ? round.roundNumber : <Link className="text-blue-600 underline" to={String(round.roundNumber)}>{round.roundNumber}</Link>}</td>
          <td className="p-8">{round.winner ?? 'Unavailable'}</td>
          <td className="p-8">{round.startTick ?? 'Unavailable'}</td>
          <td className="p-8">{round.endTick ?? 'Unavailable'}</td>
          <td className="p-8 tabular-nums">{round.combat ? `${round.combat.ctPlayers} / ${round.combat.tPlayers}${round.combat.unassignedPlayers ? ` (${round.combat.unassignedPlayers} unassigned)` : ''}` : 'Unavailable'}</td>
          <td className="p-8 text-right tabular-nums">{round.combat?.enemyKills ?? '—'}</td>
        </tr>)}</tbody></table>
      </div>}
    {selected && <Link to=".." relative="path" className="mt-16 text-blue-600 underline">All rounds</Link>}
  </Content>;
}

export const Round = Rounds;
