import { NextResponse, type NextRequest } from "next/server";
import { submitAnswer } from "~/server/actions";

export async function POST(
  request: NextRequest,
  {}: { params: { gameId: string } },
) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let requestBody: any;
  try {
    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
    requestBody = await request.json();
  } catch (error) {
    return new NextResponse("Invalid JSON", { status: 400 });
  }

  // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-member-access
  const player: string = requestBody?.player;
  if (!player) {
    return new NextResponse("Missing player field", { status: 400 });
  }
  // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-member-access
  const answer: string = requestBody?.answer;
  if (!answer) {
    return new NextResponse("Missing answer field", { status: 400 });
  }

  try {
    await submitAnswer(player, answer);
    return new NextResponse("", { status: 200 });
  } catch (error) {
    return new NextResponse((error as Error).message, {
      status: 400,
    });
  }
}
