import { useMemo } from 'react';
import { Game, TeamNumber } from 'csdm/common/types/counter-strike';
import type { Match } from 'csdm/common/types/match';
import type { Round } from 'csdm/common/types/round';
import type { MatchDetail, RecordedRound, ViewerRoundMeta } from './match-data';
import { matchRouteKey } from './match-data';
import { useSourceMatch } from './match-loader';

export function nativeSide(side: string | null): TeamNumber {
  if (side === 'CT') return TeamNumber.CT;
  if (side === 'T' || side === 'TERRORIST') return TeamNumber.T;
  return TeamNumber.UNASSIGNED;
}

function scoreBefore(detail: MatchDetail, number: number, letter: 'A' | 'B'): number {
  let score = 0;
  for (const round of detail.rounds) {
    if (round.roundNumber >= number) break;
    const side = letter === 'A' ? round.teamASide : round.teamBSide;
    if (!side || !round.winner) return Number.NaN;
    if (side === round.winner) score++;
  }
  return score;
}

export function originalRound(detail: MatchDetail, recorded: RecordedRound, meta?: ViewerRoundMeta): Round {
  const start = meta?.startTick ?? recorded.startTick ?? 0;
  const end = meta?.endTick ?? recorded.endTick ?? start;
  // Only fields consumed by the pinned CSDM viewer are adapted. In particular,
  // the native end tick is not represented as an independently observed
  // round_officially_ended event.
  return {
    number: recorded.roundNumber,
    startTick: start,
    freezetimeEndTick: recorded.freezeEndTick ?? start,
    endTick: end,
    endOfficiallyTick: end,
    teamASide: nativeSide(recorded.teamASide),
    teamBSide: nativeSide(recorded.teamBSide),
    teamAScore: scoreBefore(detail, recorded.roundNumber, 'A'),
    teamBScore: scoreBefore(detail, recorded.roundNumber, 'B'),
    winnerSide: recorded.winner ? nativeSide(recorded.winner) : null,
  } as Round;
}

// The original Viewer2D, RoundsBar, and playback controls call this hook.
// Keep its source truthful: only the fields consumed by those components are
// supplied, and unknown team/economy fields are never invented.
export function useCurrentMatch(): Match {
  const detail = useSourceMatch();
  return useMemo(() => ({
    checksum: matchRouteKey(detail.summary.id),
    demoFilePath: detail.summary.demoFilePath ?? '',
    game: Game.CS2,
    mapName: detail.summary.mapName,
    tickrate: detail.summary.tickRate ?? 64,
    rounds: detail.rounds.map((round) => originalRound(detail, round)),
    teamA: { name: 'Team A' },
    teamB: { name: 'Team B' },
  } as Match), [detail]);
}
