import React from 'react';
import { Link, useParams } from 'react-router';
import { Content } from 'csdm/ui/components/content';

function UnsupportedMatchView() {
  const { checksum } = useParams();
  return <Content>
    <div className="flex max-w-[560px] flex-col gap-y-8">
      <h1 className="text-title">This match analysis is still being ported</h1>
      <p className="text-gray-700">The selected DemoStrike library does not contain the fields required by this CS Demo Manager screen. No statistics are estimated or filled with zero.</p>
      <Link to={`/matches/${checksum ?? ''}`} className="text-blue-600 underline">Back to recorded match</Link>
    </div>
  </Content>;
}

export const MatchHeatmap = UnsupportedMatchView;
export const MatchDuels = UnsupportedMatchView;
export const OpeningDuelsStats = UnsupportedMatchView;
export const OpeningDuelsMapLoader = UnsupportedMatchView;
export const PlayersDuelsMatrix = UnsupportedMatchView;
export const Weapons = UnsupportedMatchView;
export const Viewer2DLoader = UnsupportedMatchView;
export const VideoLoader = UnsupportedMatchView;
export const ChatMessages = UnsupportedMatchView;
export const Economy = UnsupportedMatchView;
export const GrenadesFinderLoader = UnsupportedMatchView;
