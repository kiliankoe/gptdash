import type { NextRequest } from "next/server";
import { getGameState } from "~/server/actions";

export const dynamic = "force-dynamic";

export async function GET(
  request: NextRequest,
  { params }: { params: { gameId: string } },
) {
  try {
    const game = await getGameState(params.gameId);
    const gameWithoutIDs = {
      ...game,
      players: game.players.map(({ name, points }) => ({
        name,
        points,
      })),
    };
    return Response.json(gameWithoutIDs);
  } catch (error) {
    return new Response("Game not found", { status: 404 });
  }
}
