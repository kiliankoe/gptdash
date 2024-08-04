import { notFound } from "next/navigation";
import { getGameState } from "~/server/actions";

export default async function GamePage({
  params,
}: {
  params: { gameId: string };
}) {
  const game = await getGameState(params.gameId);
  if (!game) return notFound();
  switch (game.status) {
    case "waitingToStart":
      return <div>Waiting to start</div>;
    case "waitingForPrompt":
      return <div>Waiting for prompt</div>;
    case "prompting":
      return <div>Prompting</div>;
    case "voting":
      return <div>Voting</div>;
    case "leaderboard":
      return <div>Leaderboard</div>;
  }
}
