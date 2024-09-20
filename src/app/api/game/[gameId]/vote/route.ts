import { NextResponse } from "next/server";
import { voteAnswer } from "~/server/actions";

export async function POST(
  request: Request,
  { params }: { params: { gameId: string } },
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
  const answerAuthor: string = requestBody?.answerAuthor;
  if (!answerAuthor) {
    return new NextResponse("Missing answerAuthor field", { status: 400 });
  }
  // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-member-access
  const voteAuthor: string = requestBody?.voteAuthor;
  if (!voteAuthor) {
    return new NextResponse("Missing voteAuthor field", { status: 400 });
  }

  try {
    await voteAnswer(answerAuthor, voteAuthor);
    return new Response("", { status: 200 });
  } catch (error) {
    return new Response((error as Error).message, {
      status: 400,
    });
  }
}
