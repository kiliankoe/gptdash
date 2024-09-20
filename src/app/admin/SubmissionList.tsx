import Button from "../components/Button";
import { useGame } from "../components/GameProvider";

export default function SubmissionList() {
  // TODO: Allow editing of submissions for typo fixes, maybe also AI?
  const { game } = useGame();
  if (!game) return <div>Lade Spiel...</div>;
  return (
    <div>
      <h2>Antworten</h2>
      {game.rounds[game.rounds.length - 1]?.submissions.map((s) => (
        <div key={s.author}>
          {s.author}: {s.answer}
        </div>
      ))}
      <Button
        onClick={() =>
          fetch("/api/game/ds24/status", {
            method: "POST",
            body: JSON.stringify({ status: "voting" }),
          })
        }
      >
        Runde schließen
      </Button>
    </div>
  );
}
