import { check } from "k6";
import ws from "k6/ws";

// Common browser User-Agents
const userAgents = [
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0 Safari/537.36",
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 13_2) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0 Safari/537.36",
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:122.0) Gecko/20100101 Firefox/122.0",
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 13.2; rv:122.0) Gecko/20100101 Firefox/122.0",
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 13_2_1) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15",
];

const participantCount = 6000;
const socketTimeout = 60000;
const globalLogLevel = 1;

export const options = {
  vus: participantCount, // number of audience members
  duration: "10m",
};

function log(msg, logLevel = 0) {
  if (logLevel >= globalLogLevel) {
    console.log(msg);
  }
}

// Random string generator
function randomString(len) {
  const chars = "abcdefghijklmnopqrstuvwxyz0123456789";
  let out = "";
  for (let i = 0; i < len; i++) {
    out += chars[Math.floor(Math.random() * chars.length)];
  }
  return out;
}

// Build random audience token
function randomAudienceToken() {
  const part1 = randomString(8);
  const part2 = randomString(10);
  return `voter_${part1}_${part2}`;
}

// Pick a User-Agent
function randomUserAgent() {
  return userAgents[Math.floor(Math.random() * userAgents.length)];
}

export default function () {
  const token = randomAudienceToken();

  const url = `ws://127.0.0.1:6573/ws?role=audience&token=${token}`;

  const headers = {
    "User-Agent": randomUserAgent(),
    Origin: "http://127.0.0.1:6573",
  };

  const res = ws.connect(url, { headers }, (socket) => {
    socket.on("open", () => {
      log(`VU ${__VU} connected with token ${token}`);
    });

    socket.on("message", (msg) => {
      log(`VU ${__VU} received: ${msg}`);

      // Try to parse incoming message (k6 ws delivers string)
      let data;
      try {
        data = JSON.parse(msg);
      } catch (e) {
        log("Not valid JSON: " + data);
        return;
      }

      if (
        !data ||
        data.t !== "submissions" ||
        !Array.isArray(data.list) ||
        data.list.length < 1
      ) {
        return;
      }

      // Pick two random (distinct if possible) submission ids
      const n = data.list.length;
      const idxA = Math.floor(Math.random() * n);
      let idxB = Math.floor(Math.random() * n);
      if (n > 1) {
        while (idxB === idxA) idxB = Math.floor(Math.random() * n);
      }

      const vote = {
        t: "vote",
        voter_token: token,
        ai: data.list[idxA].id,
        funny: data.list[idxB].id,
        msg_id: "msg_" + randomString(12),
      };

      const delayMs = Math.floor(Math.random() * 10001); // 0..10000 ms
      setTimeout(() => {
        try {
          socket.send(JSON.stringify(vote));
          log(
            `VU ${__VU} sent vote after ${delayMs}ms: ${JSON.stringify(vote)}`,
            1,
          );
        } catch (e) {
          console.error(`VU ${__VU} failed to send vote:`, e);
        }
      }, delayMs);
    });

    socket.on("error", (e) => {
      console.error(`VU ${__VU} error:`, e);
    });

    socket.on("close", () => {
      log(`VU ${__VU} closed`);
    });

    // Keep the WebSocket alive for the duration
    socket.setTimeout(() => {
      socket.close();
    }, socketTimeout);
  });

  check(res, {
    "status 101": (r) => r && r.status === 101,
  });
}
