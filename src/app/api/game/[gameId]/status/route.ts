import { NextResponse, type NextRequest } from "next/server";
import { startGame } from "~/server/actions";
import { isGameStatus } from "~/server/state";

export async function POST(
  request: NextRequest,
  { params }: { params: { gameId: string } },
) {
  const status = await request.text();
  if (!status) {
    return new NextResponse("Missing status", { status: 400 });
  }
  if (!isGameStatus(status)) {
    return new NextResponse("Invalid status", { status: 400 });
  }

  switch (status) {
    case "waitingToStart":
      return new NextResponse("Not implemented", { status: 501 });
    case "waitingForPrompt":
      await startGame();
      return NextResponse.json({ status: "ok" });
    case "prompting":
      return new NextResponse("Not implemented", { status: 501 });
    case "voting":
      return new NextResponse("Not implemented", { status: 501 });
    case "leaderboard":
      return new NextResponse("Not implemented", { status: 501 });
  }
}
