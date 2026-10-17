import React from 'react';
import { TeamNumber } from 'csdm/common/types/counter-strike';
import { getTeamColor } from 'csdm/ui/styles/get-team-color';
import { buildPlayerId } from 'csdm/ui/match/viewer-2d/build-player-id';
import { useViewerContext } from 'csdm/ui/match/viewer-2d/use-viewer-context';
import { useSourceMatch } from './match-loader';
import type { NativeObservedPosition } from './native-original-draw-players';

type Props = { teamNumber: TeamNumber; teamName: string; teamScore: number };

// CSDM's original panel requires per-frame money, armor and weapons that the
// native v10 stream does not record. Keep the original canvas and navigation,
// but show only observed side, alive state and health here.
export function TeamPanel({ teamNumber, teamName, teamScore }: Props) {
  const { playerPositions, currentTick, focusedPlayerId, updateFocusedPlayerId } = useViewerContext();
  const match = useSourceMatch();
  const sideKnown = teamNumber === TeamNumber.CT || teamNumber === TeamNumber.T;
  const players = sideKnown ? playerPositions.filter((player) => player.tick === currentTick && player.side === teamNumber) : [];
  return <section className="rounded-4 border border-gray-300 bg-gray-50/95" aria-label={teamName}>
    <div className="flex items-center justify-between border-b border-gray-300 px-8 py-4"
      style={{ borderBottomColor: sideKnown ? getTeamColor(teamNumber) : undefined }}>
      <strong>{teamName}{sideKnown ? ` (${teamNumber === TeamNumber.CT ? 'CT' : 'T'})` : ' (side unavailable)'}</strong>
      <span className="tabular-nums">{Number.isFinite(teamScore) ? teamScore : '?'}</span>
    </div>
    {sideKnown && players.map((player) => {
      const observedAlive = (player as NativeObservedPosition).observedAlive;
      const name = match.players.find((item) => item.steamid === player.playerSteamId)?.name ?? player.playerName;
      const id = buildPlayerId(player.playerSteamId, player.playerName);
      return <button key={id} type="button" aria-pressed={focusedPlayerId === id}
        onClick={() => updateFocusedPlayerId(id)}
        className="flex w-full items-center justify-between gap-8 border-b border-gray-300 px-8 py-4 text-left last:border-b-0 hover:bg-gray-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-500">
        <span className="truncate" title={name}>{name}</span>
        <span className="shrink-0 tabular-nums text-gray-700">{observedAlive === null ? 'Alive unavailable'
          : observedAlive === false ? 'Out'
          : Number.isFinite(player.health) ? `${player.health} HP` : 'HP unavailable'}</span>
      </button>;
    })}
    {!sideKnown && <p className="px-8 py-4 text-gray-700">GameRules phase did not establish this team side.</p>}
  </section>;
}
