import Button from "../components/Button";
import { useGame } from "../components/GameProvider";

export default function AdminVote() {
  const { game, isLoading } = useGame();
  if (isLoading) return <div>Lade Spiel...</div>;
  if (!game) return <div>Spiel nicht gefunden lol</div>;

  const allVoteCount = game.rounds[game.rounds.length - 1]?.submissions.reduce(
    (acc, s) => acc + s.supporters.length,
    0,
  );
  return (
    <div>
      <h2>Voting</h2>
      <p>{allVoteCount} Stimmen</p>
      <Button
        onClick={() => {
          fetch("/api/game/ds24/status", {
            method: "POST",
            body: JSON.stringify({ status: "leaderboard" }),
          }).catch((e) => console.error(e));
        }}
      >
        Abstimmung beenden
      </Button>
    </div>
  );
}
