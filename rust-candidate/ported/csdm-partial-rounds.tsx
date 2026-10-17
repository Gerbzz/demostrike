import React from 'react';
import { Link, useParams } from 'react-router';
import { Content } from 'csdm/ui/components/content';
import type { MatchDetail, RecordedRound, RoundDeath, RoundHurt } from './match-data';
import { useSourceMatch } from './match-loader';

// The pinned CSDM RoundsHistory and RoundEntry are organized around Team A/B,
// economy, tags, and Kill objects. Keep their history -> entry layout while
// rendering only fields the native demo import actually recorded.
const knownSide = (side: string | null) => side === 'CT' || side === 'T' ? side : null;
const recorded = (value: number | string | null) => value === null ? 'Unavailable' : value;
const roundReasonLabels: Record<string, string> = {
  t_killed: 'All T eliminated',
  ct_killed: 'All CT eliminated',
  bomb_defused: 'Bomb defused',
  bomb_exploded: 'Bomb exploded',
  time_ran_out: 'Time expired',
  ct_surrender: 'CT surrendered',
  t_surrender: 'T surrendered',
};
const roundReason = (reason: string | null) => reason === null ? 'Unavailable' : roundReasonLabels[reason] ?? reason;

function RoundsHistory({ rounds }: { rounds: RecordedRound[] }) {
  return <section aria-label="Round history" className="mt-16">
    <h2 className="text-subtitle">Round history</h2>
    <p className="mt-4 text-gray-700">CT and T are round sides. Team A/B sides appear when match start and game phase switches were observed.</p>
    <div className="mt-12 max-w-full overflow-x-auto pb-8">
      <div className="grid w-max grid-flow-col grid-rows-[30px_repeat(2,35px)] gap-px border border-gray-300 bg-gray-300"
        style={{ gridTemplateColumns: `80px repeat(${rounds.length}, 48px)` }}>
        <div className="flex items-center bg-gray-50 px-8 text-body-strong">Round</div>
        <div className="flex items-center bg-gray-50 px-8 text-body-strong">CT</div>
        <div className="flex items-center bg-gray-50 px-8 text-body-strong">T</div>
        {rounds.map((round) => <React.Fragment key={round.roundNumber}>
          <Link className="flex items-center justify-center bg-gray-50 underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-500"
            to={String(round.roundNumber)} aria-label={`Open round ${round.roundNumber}`}>{round.roundNumber}</Link>
          <div className="flex items-center justify-center bg-gray-50 text-blue-600" aria-label={knownSide(round.winner) === 'CT' ? 'CT won' : undefined}>
            {knownSide(round.winner) === 'CT' ? 'W' : null}
          </div>
          <div className="flex items-center justify-center bg-gray-50 text-orange-500" aria-label={knownSide(round.winner) === 'T' ? 'T won' : undefined}>
            {knownSide(round.winner) === 'T' ? 'W' : null}
          </div>
        </React.Fragment>)}
      </div>
    </div>
    <p className="text-gray-700">Unmarked columns have no recorded CT/T winner.</p>
  </section>;
}

function playerLabel(detail: MatchDetail, steamid: string | null) {
  if (steamid === null) return 'Unknown player';
  const player = detail.players.find((item) => item.steamid === steamid);
  return player ? player.name : steamid;
}

function DeathEntry({ death, detail }: { death: RoundDeath; detail: MatchDetail }) {
  const attacker = playerLabel(detail, death.attackerSteamid);
  const victim = playerLabel(detail, death.victimSteamid);
  return <li className="grid grid-cols-[minmax(64px,auto)_1fr] gap-x-12 border-t border-gray-300 py-8 first:border-t-0">
    <span className="tabular-nums text-gray-700">Tick {death.tick}</span>
    <span className="break-words">
      {attacker}{knownSide(death.attackerSide) && <span className="ml-4 text-gray-700">({death.attackerSide})</span>}
      <span className="mx-8 text-gray-700">&gt;</span>{victim}
      {knownSide(death.victimSide) && <span className="ml-4 text-gray-700">({death.victimSide})</span>}
      {death.weapon && <span className="ml-8 text-gray-700">- {death.weapon}</span>}
      {death.assisterSteamid && <span className="ml-8 text-gray-700">- assist: {playerLabel(detail, death.assisterSteamid)}</span>}
    </span>
  </li>;
}

function HurtEntry({ hurt, detail }: { hurt: RoundHurt; detail: MatchDetail }) {
  return <li className="grid grid-cols-[minmax(64px,auto)_1fr] gap-x-12 border-t border-gray-300 py-8 first:border-t-0">
    <span className="tabular-nums text-gray-700">Tick {hurt.tick}</span>
    <span className="break-words">
      {playerLabel(detail, hurt.attackerSteamid)}
      {knownSide(hurt.attackerSide) && <span className="ml-4 text-gray-700">({hurt.attackerSide})</span>}
      <span className="mx-8 text-gray-700">&gt;</span>{playerLabel(detail, hurt.victimSteamid)}
      {knownSide(hurt.victimSide) && <span className="ml-4 text-gray-700">({hurt.victimSide})</span>}
      <span className="ml-8 tabular-nums">{hurt.effectiveDamage === null ? 'Effective HP unavailable' : `${hurt.effectiveDamage} effective HP`}</span>
      <span className="ml-8 text-gray-700 tabular-nums">{hurt.rawDamage} raw</span>
      <span className="ml-8 text-gray-700">{hurt.weapon ?? 'Weapon unavailable'}</span>
    </span>
  </li>;
}

function RoundEntry({ round, detail, expanded }: { round: RecordedRound; detail: MatchDetail; expanded: boolean }) {
  const winner = knownSide(round.winner);
  return <details className="border-b border-gray-300 py-12 last:border-b-0" open={expanded || undefined}>
    <summary className="flex cursor-pointer flex-wrap items-center gap-x-16 gap-y-4 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-500">
      <span className="min-w-[78px] text-body-strong">Round {round.roundNumber}</span>
      <span className="min-w-[75px]">{winner ? `${winner} win` : 'Winner unavailable'}</span>
      <span className="text-gray-700">{roundReason(round.endReason)}</span>
      <span className="ml-auto text-gray-700 tabular-nums">{round.startTick ?? '?'}-{round.endTick ?? '?'} ticks</span>
    </summary>
    <div className="mt-12 ml-16 grid gap-16 border-l-2 border-gray-300 pl-12 lg:grid-cols-[minmax(250px,1fr)_minmax(300px,2fr)]">
      <dl className="grid grid-cols-2 content-start gap-x-12 gap-y-8 text-body">
        <div><dt className="text-gray-700">Start tick</dt><dd>{recorded(round.startTick)}</dd></div>
        <div><dt className="text-gray-700">Freeze end tick</dt><dd>{recorded(round.freezeEndTick)}</dd></div>
        <div><dt className="text-gray-700">End tick</dt><dd>{recorded(round.endTick)}</dd></div>
        <div><dt className="text-gray-700">End reason</dt><dd>{roundReason(round.endReason)}</dd></div>
        <div><dt className="text-gray-700">Team A side</dt><dd>{round.teamASide ?? 'Unavailable'}</dd></div>
        <div><dt className="text-gray-700">Team B side</dt><dd>{round.teamBSide ?? 'Unavailable'}</dd></div>
        <div><dt className="text-gray-700">CT equipment value</dt><dd>{recorded(round.ctEquipValue)}</dd></div>
        <div><dt className="text-gray-700">T equipment value</dt><dd>{recorded(round.tEquipValue)}</dd></div>
        <div><dt className="text-gray-700">Observed CT / T</dt><dd>{round.combat ? `${round.combat.ctPlayers} / ${round.combat.tPlayers}` : 'Unavailable'}</dd></div>
        <div><dt className="text-gray-700">Enemy eliminations</dt><dd>{recorded(round.combat?.enemyKills ?? null)}</dd></div>
      </dl>
      <div>
        <h3 className="text-body-strong">Recorded death events</h3>
        {round.deaths === null ? <p className="mt-4 text-gray-700">Ordered death events were not recorded for this import.</p> :
          round.deaths.length === 0 ? <p className="mt-4 text-gray-700">No deaths recorded in this round.</p> :
            <ol className="mt-8">{round.deaths.map((death) => <DeathEntry key={death.sourceIndex} death={death} detail={detail} />)}</ol>}
      </div>
    </div>
    {expanded && <section className="mt-16 ml-16 border-l-2 border-gray-300 pl-12" aria-label="Round raw damage">
      <h3 className="text-body-strong">Recorded damage events</h3>
      <p className="mt-4 text-gray-700">Effective HP follows the pinned CSDM damage rule when recorded. Raw values can include overkill. Hitgroups and shots fired remain unavailable.</p>
      {round.hurts === null ? <p className="mt-8 text-gray-700">Ordered damage events were not recorded for this import.</p> :
        round.hurts.length === 0 ? <p className="mt-8 text-gray-700">No damage events recorded in this round.</p> :
          <ol className="mt-8">{round.hurts.map((hurt) => <HurtEntry key={hurt.sourceIndex} hurt={hurt} detail={detail} />)}</ol>}
    </section>}
  </details>;
}

export function Rounds() {
  const detail = useSourceMatch();
  const { number } = useParams();
  const selected = number === undefined ? null : detail.rounds.find((round) => String(round.roundNumber) === number);
  if (number !== undefined && !selected) return <Content>
    <p role="alert">Round {number} is not recorded in this library.</p>
    <Link to=".." relative="path" className="mt-8 inline-block text-blue-600 underline">All rounds</Link>
  </Content>;
  const selectedIndex = selected ? detail.rounds.findIndex((round) => round.roundNumber === selected.roundNumber) : -1;
  return <Content>
    <div className="flex flex-wrap items-center justify-between gap-12">
      <h1 className="text-title">{selected ? `Round ${selected.roundNumber}` : 'Recorded rounds'}</h1>
      {selected && <nav aria-label="Round navigation" className="flex flex-wrap items-center gap-12">
        {selectedIndex > 0 && <Link to={'../' + detail.rounds[selectedIndex - 1].roundNumber} relative="path" className="text-blue-600 underline">Previous round</Link>}
        <Link to=".." relative="path" className="text-blue-600 underline">All rounds</Link>
        {selectedIndex < detail.rounds.length - 1 && <Link to={'../' + detail.rounds[selectedIndex + 1].roundNumber} relative="path" className="text-blue-600 underline">Next round</Link>}
      </nav>}
    </div>
    <p className="mt-4 max-w-[760px] text-gray-700">The history follows the original CSDM round view. Recorded winners, ticks, Team A/B sides and ordered combat events appear where observed. Tags, full economy and playback remain unavailable.</p>
    {detail.rounds.length === 0 ? <p className="mt-16" role="status">No rounds were recorded for this match.</p> : <>
      {!selected && <RoundsHistory rounds={detail.rounds} />}
      <section className="mt-16" aria-label="Round entries">
        <h2 className="text-subtitle">{selected ? 'Round details' : 'Round by round'}</h2>
        <div className="mt-8 border-t border-gray-300">{(selected ? [selected] : detail.rounds).map((round) =>
          <RoundEntry key={round.roundNumber} round={round} detail={detail} expanded={Boolean(selected)} />)}</div>
      </section>
    </>}
  </Content>;
}

export const Round = Rounds;
