import type { NextRequest } from "next/server";
import { getGameState } from "~/server/actions";

export const dynamic = "force-dynamic";

export async function GET(
  request: NextRequest,
  { params }: { params: { gameId: string } },
) {
  try {
    const game = await getGameState(params.gameId);
    return Response.json(game);
  } catch (error) {
    return new Response("Game not found", { status: 404 });
  }
}
