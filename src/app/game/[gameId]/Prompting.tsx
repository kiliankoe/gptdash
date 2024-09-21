"use client";

import { useState } from "react";
import Button from "~/app/components/Button";
import { useGame } from "~/app/components/GameProvider";
import usePlayer from "~/app/components/usePlayer";

export default function Prompting() {
  const { game, isLoading } = useGame();
  const player = usePlayer();
  const [answer, setAnswer] = useState<string>("");
  const [hasSubmitted, setHasSubmitted] = useState(false);

  if (isLoading) return <div>Lade Spiel...</div>;
  if (!game) {
    return <div>Spiel nicht gefunden, probier&apos;s mal mit F5 🤔</div>;
  }

  const handleSubmit = () => {
    fetch("/api/game/ds24/submit", {
      method: "POST",
      body: JSON.stringify({ player, answer }),
    }).catch((e) => console.error(e));
    setHasSubmitted(true);
  };

  return (
    <div className="flex flex-col gap-y-2">
      <h2 className="font-bold">
        {game.rounds[game.rounds.length - 1]?.prompt}
      </h2>

      <textarea
        placeholder="Deine Antwort"
        value={answer}
        onChange={(e) => setAnswer(e.target.value)}
        className="h-[200px] w-full bg-slate-800 p-2 text-lg text-white"
      />
      <p>Antworte bitte in drei kurzen Sätzen Fließtext.</p>

      <Button onClick={handleSubmit} disabled={answer.length <= 15}>
        {hasSubmitted ? "Nochmal absenden" : "Absenden"}
      </Button>
    </div>
  );
}
