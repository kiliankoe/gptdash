"use client";

import { useGame } from "./GameProvider";

export default function DevFooter() {
  const { game } = useGame();
  return (
    <footer className="absolute bottom-0 left-0 right-0 text-center">
      <code className="text-[10px] text-white">
        {JSON.stringify(game, null, 2)}
      </code>
    </footer>
  );
}
