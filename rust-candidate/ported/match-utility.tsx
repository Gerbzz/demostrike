import React, { useMemo, useState } from 'react';
import { Outlet } from 'react-router';
import { Content } from 'csdm/ui/components/content';
import { GrenadesTabs } from 'csdm/ui/match/grenades/grenades-tabs';
import { useSourceMatch } from './match-loader';

export function MatchGrenades() {
  return <><GrenadesTabs /><Outlet /></>;
}

export function GrenadesStats() {
  const { utilityEvents } = useSourceMatch();
  const [search, setSearch] = useState('');
  const visible = useMemo(() => utilityEvents.filter((event) =>
    `${event.utilityType} ${event.throwerName} ${event.throwerTeam} ${event.roundNumber}`
      .toLocaleLowerCase().includes(search.toLocaleLowerCase())), [utilityEvents, search]);

  return <Content>
    <div className="flex flex-wrap items-end justify-between gap-8">
      <div>
        <h1 className="text-title">Recorded utility</h1>
        <p className="mt-4 text-gray-700">These events come from the DemoStrike import. Blind and damage totals appear only when attribution is verified.</p>
      </div>
      <label className="flex items-center gap-x-8">Find utility
        <input aria-label="Find utility" className="rounded-4 border border-gray-400 bg-gray-75 p-8"
          value={search} onChange={(event) => setSearch(event.target.value)} />
      </label>
    </div>
    {utilityEvents.length === 0 ? <p className="mt-16">No utility events were recorded for this match.</p> :
      <div className="mt-16 overflow-auto rounded-8 border border-gray-300">
        <table className="w-full text-left"><thead className="bg-gray-100"><tr>
          <th className="p-8">Round</th><th className="p-8">Type</th><th className="p-8">Thrower</th>
          <th className="p-8">Side</th><th className="p-8">Tick</th>
          <th className="p-8">Enemies blinded</th><th className="p-8">Enemy damage</th>
        </tr></thead><tbody>{visible.map((event) => <tr key={event.id} className="border-t border-gray-300">
          <td className="p-8">{event.roundNumber}</td><td className="p-8">{event.utilityType}</td>
          <td className="p-8">{event.throwerName}</td><td className="p-8">{event.throwerTeam}</td>
          <td className="p-8">{event.throwTick}</td>
          <td className="p-8">{event.enemiesBlinded ?? 'Unavailable'}</td>
          <td className="p-8">{event.damageEnemy ?? 'Unavailable'}</td>
        </tr>)}</tbody></table>
        {visible.length === 0 && <p className="p-12">No utility found. Try another search.</p>}
      </div>}
  </Content>;
}
