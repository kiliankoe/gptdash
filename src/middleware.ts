import type { NextRequest } from "next/server";
import { NextResponse } from "next/server";

const [AUTH_USER, AUTH_PASS] = (process.env.HTTP_BASIC_AUTH ?? ":").split(":");

export function middleware(req: NextRequest) {
  if (!isAuthenticated(req)) {
    return new NextResponse("Authentication required", {
      status: 401,
      headers: { "WWW-Authenticate": "Basic" },
    });
  }

  return NextResponse.next();
}

function isAuthenticated(req: NextRequest) {
  const authheader =
    req.headers.get("authorization") ?? req.headers.get("Authorization");

  if (!authheader) {
    return false;
  }

  const base64Credentials = authheader.split(" ")[1];
  if (!base64Credentials) {
    return false;
  }
  const auth = Buffer.from(base64Credentials, "base64").toString().split(":");
  const user = auth[0];
  const pass = auth[1];

  if (user == AUTH_USER && pass == AUTH_PASS) {
    return true;
  } else {
    return false;
  }
}

export const config = {
  matcher: ["/admin/:path*", "/api/game/:gameId/status"],
};
