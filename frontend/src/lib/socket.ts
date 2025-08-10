import io from "socket.io-client";

// Using 'any' to avoid TS type friction between client versions
let socket: any = null;

export function getSocket() {
  if (!socket) {
    socket = io(import.meta.env.VITE_API_URL || window.location.origin, {
      transports: ["websocket", "polling"],
      autoConnect: true,
    });
    // Diagnostics
    socket.on("connect", () => console.log("[socket] connected", socket!.id));
    socket.on("disconnect", (reason: any) => console.log("[socket] disconnect", reason));
    socket.on("connect_error", (err: any) => console.warn("[socket] connect_error", (err as any)?.message || err));
    socket.on("reconnect_attempt", (n: number) => console.log("[socket] reconnect_attempt", n));
    socket.on("reconnect_error", (err: any) => console.warn("[socket] reconnect_error", (err as any)?.message || err));
    socket.on("connect", () => {
      // try to resume if we have tokens
      const sessionCode = localStorage.getItem("sessionCode");
      const hostToken = localStorage.getItem("hostToken");
      const playerToken = localStorage.getItem("playerToken");
      if (sessionCode && (hostToken || playerToken)) {
        const role = hostToken ? "host" : "player";
        const token = hostToken || playerToken!;
        socket!.emit("game:resume", { sessionCode, role, token }, (res: any) => {
          if (res?.error) {
            console.warn("[socket] resume error:", res.error);
            // Clear invalid tokens and redirect
            localStorage.removeItem("sessionCode");
            localStorage.removeItem("hostToken");
            localStorage.removeItem("playerToken");
            localStorage.removeItem("playerId");
            // Force page reload to redirect properly
            window.location.href = "/";
          } else {
            console.log("[socket] successfully resumed session");
          }
        });
      }
    });
  }
  return socket;
}
