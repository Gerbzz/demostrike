import React, { useMemo } from 'react';
import { useViewerContext } from 'csdm/ui/match/viewer-2d/use-viewer-context';
import type { MatchDetail, RecordedRound, ViewerRoundMeta } from './match-data';

type RecordedEvent = { key: string; tick: number; kind: string; label: string };

function eventRows(detail: MatchDetail, round: RecordedRound, meta: ViewerRoundMeta): RecordedEvent[] {
  const names = new Map(detail.players.map((player) => [player.steamid, player.name]));
  const player = (steamid: string | null) => steamid === null ? 'world' : names.get(steamid) ?? steamid;
  const rows: RecordedEvent[] = [];
  for (const event of meta.utilityEvents) {
    if (event.detonateTick === null || event.detonateTick < meta.startTick! || event.detonateTick > meta.endTick!) continue;
    rows.push({ key: `utility-${event.id}`, tick: event.detonateTick, kind: 'grenade',
      label: `${event.utilityType} detonated · ${event.throwerName}` });
  }
  for (const hurt of round.hurts ?? []) {
    if (hurt.tick < meta.startTick! || hurt.tick > meta.endTick!) continue;
    const amount = hurt.effectiveDamage === null ? `${hurt.rawDamage} raw damage` : `${hurt.effectiveDamage} HP`;
    rows.push({ key: `hurt-${hurt.sourceIndex}`, tick: hurt.tick, kind: 'damage',
      label: `${player(hurt.attackerSteamid)} → ${player(hurt.victimSteamid)} · ${amount}` });
  }
  for (const death of round.deaths ?? []) {
    if (death.tick < meta.startTick! || death.tick > meta.endTick!) continue;
    rows.push({ key: `death-${death.sourceIndex}`, tick: death.tick, kind: 'death',
      label: `${player(death.attackerSteamid)} killed ${player(death.victimSteamid)}` });
  }
  if (meta.bombEventsAvailable) {
    for (const event of meta.bombEvents) {
      const outsideReplay = event.tick < meta.startTick! || event.tick > meta.endTick!;
      rows.push({ key: `bomb-${event.sourceIndex}`, tick: event.tick, kind: 'bomb',
        label: `${event.kind.replace('bomb_', 'Bomb ')} · ${event.actorName ?? 'actor unavailable'} · site unavailable${outsideReplay ? ' · between rounds' : ''}` });
    }
  }
  return rows.sort((a, b) => a.tick - b.tick || a.key.localeCompare(b.key));
}

export function NativeOriginalEventList({ detail, recorded, meta }: {
  detail: MatchDetail; recorded: RecordedRound; meta: ViewerRoundMeta;
}) {
  const { pause, setCurrentTick, currentTick } = useViewerContext();
  const rows = useMemo(() => eventRows(detail, recorded, meta), [detail, recorded, meta]);
  return <details className="native-recorded-events absolute bottom-96 right-8 z-20 w-[230px] rounded-4 border border-gray-300 bg-gray-50/95 text-body shadow-lg"
    data-recorded-event-count={rows.length}>
    <summary className="cursor-pointer px-8 py-4 font-semibold">Recorded events ({rows.length})</summary>
    <div className="max-h-[300px] overflow-y-auto border-t border-gray-300">
      {!meta.bombEventsAvailable && <p className="px-8 py-4 text-gray-700">Bomb events unavailable for this import.</p>}
      {recorded.hurts === null && <p className="px-8 py-4 text-gray-700">Damage events unavailable for this import.</p>}
      {rows.length === 0 && <p className="px-8 py-4 text-gray-700">No recorded events in this round.</p>}
      {rows.map((event) => <button key={event.key} type="button"
        data-event-kind={event.kind} data-event-tick={event.tick}
        disabled={event.tick < meta.startTick! || event.tick > meta.endTick!}
        aria-current={event.tick === currentTick ? 'true' : undefined}
        className="flex w-full gap-8 border-b border-gray-300 px-8 py-4 text-left last:border-b-0 hover:bg-gray-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-500 disabled:cursor-not-allowed disabled:opacity-60"
        onClick={() => { pause(); setCurrentTick(event.tick); }}>
        <span className="w-48 shrink-0 tabular-nums text-gray-700">{event.tick}</span>
        <span className="min-w-0 break-words">{event.label}</span>
      </button>)}
      <p className="px-8 py-4 text-gray-700">Smoke effect area and expiration, inferno cells, flash blindness and bombsite names are unavailable.</p>
    </div>
  </details>;
}
