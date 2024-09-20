import { NextResponse } from "next/server";
import { setAppState } from "~/server/actions";
import { appState, type AppState } from "~/server/state";

export async function GET() {
  return NextResponse.json(appState);
}

export async function POST(request: Request) {
  const body: AppState = await request.json();
  setAppState(body);
  return new NextResponse("", { status: 200 });
}
