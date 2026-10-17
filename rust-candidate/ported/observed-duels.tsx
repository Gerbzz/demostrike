import React, { useState } from 'react';
import { Link } from 'react-router';
import { Content } from 'csdm/ui/components/content';
import { useSourceMatch } from './match-loader';

// The original MatchDuels TabLinks and nested route remain in place. This
// adapter reads only event identities verified by the native demo import.
export function PlayersDuelsMatrix() {
  const detail = useSourceMatch();
  const [roundFilter, setRoundFilter] = useState('all');
  const [selectedPair, setSelectedPair] = useState<string | null>(null);
  const available = detail.rounds.some((round) => round.deaths !== null);
  const selectedRounds = roundFilter === 'all'
    ? detail.rounds : detail.rounds.filter((round) => String(round.roundNumber) === roundFilter);
  const observed = selectedRounds.flatMap((round) =>
    (round.deaths ?? []).map((death) => ({ death, roundNumber: round.roundNumber })));
  const duels = observed.filter(({ death }) =>
    death.attackerSteamid !== null &&
    death.attackerSteamid !== death.victimSteamid &&
    (death.attackerSide === 'CT' && death.victimSide === 'T' ||
      death.attackerSide === 'T' && death.victimSide === 'CT'));
  const players = detail.players;
  const name = (steamid: string) => players.find((player) => player.steamid === steamid)?.name ?? steamid;
  const pairKey = (attacker: string, victim: string) => `${attacker}:${victim}`;
  const pairEvents = selectedPair === null ? [] : duels.filter(({ death }) =>
    pairKey(death.attackerSteamid!, death.victimSteamid) === selectedPair);
  const selectedNames = selectedPair === null ? null : (() => {
    const event = pairEvents[0]?.death;
    return event ? `${name(event.attackerSteamid!)} → ${name(event.victimSteamid)}` : 'Selected duel';
  })();

  return <Content>
    <div className="flex flex-wrap items-end justify-between gap-12">
      <div>
        <h1 className="text-title">Recorded duels</h1>
        <p className="mt-4 max-w-[760px] text-gray-700">Rows are attackers; columns are victims. Counts include only opposing sides observed at each death. Stable Team A/B labels are shown only when the match start and phase switches were observed.</p>
      </div>
      <label className="flex items-center gap-8 text-body-strong">Round
        <select aria-label="Round filter" className="rounded border border-gray-400 bg-gray-50 px-8 py-4 text-body"
          value={roundFilter} onChange={(event) => { setRoundFilter(event.target.value); setSelectedPair(null); }}>
          <option value="all">All recorded rounds</option>
          {detail.rounds.map((round) => <option key={round.roundNumber} value={round.roundNumber}>Round {round.roundNumber}</option>)}
        </select>
      </label>
    </div>
    {!available ? <p className="mt-16" role="status">Ordered death events are unavailable for this import. The duel matrix cannot be calculated.</p> :
      players.length === 0 ? <p className="mt-16" role="status">No players were recorded for this match.</p> :
      <section className="mt-16" aria-label="Observed duel matrix">
        <div className="max-w-full overflow-x-auto rounded border border-gray-300">
          <table className="w-full min-w-max border-collapse text-body">
            <caption className="sr-only">Opposing-side death events by attacker and victim</caption>
            <thead><tr className="bg-gray-100">
              <th scope="col" className="sticky left-0 border-b border-r border-gray-300 bg-gray-100 px-12 py-8 text-left">Attacker / victim</th>
              {players.map((player) => <th scope="col" key={player.steamid} title={player.name}
                className="max-w-[110px] border-b border-gray-300 px-8 py-8 text-center">
                <span className="block truncate">{player.name}</span></th>)}
            </tr></thead>
            <tbody>{players.map((attacker) => <tr key={attacker.steamid} className="border-t border-gray-200">
              <th scope="row" title={attacker.name} className="sticky left-0 max-w-[160px] border-r border-gray-300 bg-gray-50 px-12 py-8 text-left">
                <span className="block truncate">{attacker.name}</span></th>
              {players.map((victim) => {
                const pair = pairKey(attacker.steamid, victim.steamid);
                const count = duels.filter(({ death }) =>
                  death.attackerSteamid === attacker.steamid && death.victimSteamid === victim.steamid).length;
                return <td key={victim.steamid} className="min-w-[72px] px-4 py-4 text-center tabular-nums">
                  <button type="button" disabled={count === 0} onClick={() => setSelectedPair(pair)}
                    aria-label={`${attacker.name} eliminated ${victim.name}: ${count}`}
                    aria-pressed={selectedPair === pair}
                    className="w-full rounded px-8 py-4 enabled:text-blue-600 enabled:hover:bg-blue-50 enabled:underline enabled:focus-visible:outline enabled:focus-visible:outline-2 enabled:focus-visible:outline-blue-500 disabled:text-gray-500">
                    {count}
                  </button>
                </td>;
              })}
            </tr>)}</tbody>
          </table>
        </div>
        <p className="mt-8 text-gray-700">Self deaths, same-side deaths, and events without both observed sides are excluded. Raw event rows remain available in Round details.</p>
      </section>}
    {selectedPair !== null && <section className="mt-20" aria-label="Selected duel events">
      <h2 className="text-subtitle">{selectedNames}</h2>
      {pairEvents.length === 0 ? <p className="mt-8" role="status">No observed duels in this round filter.</p> :
        <ol className="mt-8 divide-y divide-gray-300">{pairEvents.map(({ death, roundNumber }) =>
          <li key={death.sourceIndex} className="flex flex-wrap items-center gap-x-12 gap-y-4 py-8">
            <Link className="text-blue-600 underline" to={`../rounds/${roundNumber}`} relative="path">Round {roundNumber}</Link>
            <span className="tabular-nums">Tick {death.tick}</span>
            <span className="text-gray-700">{death.weapon ?? 'Weapon unavailable'}</span>
            {death.assisterSteamid && <span className="text-gray-700">Assist: {name(death.assisterSteamid)}</span>}
          </li>)}</ol>}
    </section>}
  </Content>;
}
