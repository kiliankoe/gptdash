"use client";

import PlayerList from "~/app/components/PlayerList";

export function WaitingToStart() {
  return (
    <div className="">
      <h2 className="mb-8 text-2xl">Warte auf Spielstart</h2>
      <PlayerList />
    </div>
  );
}
