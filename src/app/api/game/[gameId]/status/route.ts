import { NextResponse, type NextRequest } from "next/server";
import { startGame, startNewRound } from "~/server/actions";
import { isGameStatus } from "~/server/state";

export async function POST(
  request: NextRequest,
  { params }: { params: { gameId: string } },
) {
  // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
  const requestBody = await request.json();

  // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-member-access
  const status: string = requestBody?.status;
  if (!requestBody || !status) {
    return new NextResponse("Missing body or status field", { status: 400 });
  }
  if (!isGameStatus(status)) {
    return new NextResponse("Invalid status", { status: 400 });
  }

  switch (status) {
    case "waitingToStart":
      return new NextResponse("Not implemented", { status: 501 });
    case "waitingForPrompt":
      await startGame();
      return new NextResponse("", { status: 200 });
    case "prompting":
      // This check might get in the way for now, but should be done in the future.
      // if (appState.games.ds24?.status !== "waitingForPrompt") {
      //   return new NextResponse("Game is not waiting for prompt", {
      //     status: 400,
      //   });
      // }
      // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-member-access
      const prompt: string = requestBody?.prompt;
      await startNewRound(prompt);
      return new NextResponse("", { status: 200 });
    case "voting":
      return new NextResponse("Not implemented", { status: 501 });
    case "leaderboard":
      return new NextResponse("Not implemented", { status: 501 });
  }
}
