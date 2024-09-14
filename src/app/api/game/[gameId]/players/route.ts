import { NextResponse, type NextRequest } from "next/server";
import { addPlayer } from "~/server/actions";

export const dynamic = "force-dynamic";

export async function POST(
  request: NextRequest,
  { params }: { params: { gameId: string } },
) {
  // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
  const name = await request.text();
  if (!name) {
    return new NextResponse("Missing name", { status: 400 });
  }
  try {
    const playerId = await addPlayer(name, params.gameId);
    return NextResponse.json({ playerId });
  } catch (error) {
    if (error instanceof Error) {
      return new NextResponse(error.message, { status: 400 });
    }
    return new NextResponse("Unknown error", { status: 500 });
  }
}
