"use client";

import { useMemo, useRef } from "react";
import Button from "~/app/components/Button";
import { useGame } from "~/app/components/GameProvider";
import usePlayer from "~/app/components/usePlayer";
import type { Submission } from "~/server/state";

function shuffleArray<T>(array: T[]): T[] {
  for (let i = array.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [array[i], array[j]] = [array[j]!, array[i]!];
  }
  return array;
}

export default function Voting() {
  const { game, isLoading } = useGame();
  const player = usePlayer();

  const allSubmissions = game?.rounds[game.rounds.length - 1]?.submissions;

  const shuffledSubmissionsRef = useRef<Submission[] | null>(null);

  const submissions = useMemo(() => {
    if (!allSubmissions) return [];
    if (!shuffledSubmissionsRef.current) {
      shuffledSubmissionsRef.current = shuffleArray([...allSubmissions]);
    }
    return shuffledSubmissionsRef.current;
  }, [allSubmissions]);

  if (isLoading) return <div>Lade Spiel...</div>;
  if (!game)
    return <div>Spiel nicht gefunden, probier&apos;s mal mit F5 🤔</div>;

  return (
    <div>
      <h2>Wähle die Antwort der KI aus!</h2>
      <ul>
        {submissions.map((s) => (
          <li key={s.author} className="flex flex-col gap-y-2">
            {s.answer}
            <Button
              onClick={() =>
                fetch("/api/game/ds24/vote", {
                  method: "POST",
                  body: JSON.stringify({
                    answerAuthor: s.author,
                    voteAuthor: player,
                  }),
                })
              }
            >
              ↑ Für diese Antwort abstimmen
            </Button>
          </li>
        ))}
      </ul>
    </div>
  );
}
