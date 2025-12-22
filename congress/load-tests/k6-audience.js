import ws from "k6/ws";
import { check } from "k6";
import { sha256 } from "k6/crypto";
import { sleep } from "k6";

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

function biasedIndexByPower(n, alpha = 2) {
  // produce a value in [0,1) biased toward 0 when alpha>1
  const u = Math.random();
  const v = Math.pow(u, alpha);
  return Math.floor(v * n);
}

export default function () {
  const token = randomAudienceToken();

  const url = `ws://127.0.0.1:6573/ws?role=audience&token=${token}`;

  const headers = {
    "User-Agent": randomUserAgent(),
    Origin: "http://127.0.0.1:6573",
  };

  var nonce;

  const res = ws.connect(url, { headers }, function (socket) {
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

      if (!data) {
        log("not valid data?");
        return;
      }

      if (data.t === "vote_challenge") {
        nonce = data.nonce;
        log("Updated nonce to " + nonce);
      }

      if (data.t === "submissions") {
        // Pick two random (distinct if possible) submission ids
        const n = data.list.length;
        const idxA = biasedIndexByPower(n, 2);
        let idxB = biasedIndexByPower(n, 0.5);
        if (n > 1) {
          while (idxB === idxA) idxB = Math.floor(Math.random() * n);
        }

        const vote = {
          t: "vote",
          voter_token: token,
          ai: data.list[idxA].id,
          funny: data.list[idxB].id,
          msg_id: "msg_" + randomString(12),
          challenge_nonce: nonce,
          challenge_response: sha256(nonce + token, "hex").slice(0, 16),
          is_webdriver: false,
        };

        const delayMs = Math.floor(500 + Math.random() * 10001); // 0..10000 ms
        sleep(delayMs / 1000);
        try {
          socket.send(JSON.stringify(vote));
          log(
            `VU ${__VU} sent vote after ${delayMs}ms: ${JSON.stringify(vote)}`,
            1,
          );
        } catch (e) {
          console.error(`VU ${__VU} failed to send vote:`, e);
        }

        // send vote back; use socket.send or socket.send(JSON.stringify(vote)) depending on server expectations
        // try {
        //   socket.send(JSON.stringify(vote));
        //   log(`VU ${__VU} sent vote: ${JSON.stringify(vote)}`, 1);
        // } catch (e) {
        //   console.error(`VU ${__VU} failed to send vote:`, e);
        // }
      }

      if (data.t === "trivia_question") {
        // Pick two random (distinct if possible) submission ids
        const n = data.choices.length;
        const idxA = biasedIndexByPower(n, 2);

        const vote = {
          t: "submit_trivia_vote",
          voter_token: token,
          choice_index: idxA,
          is_webdriver: false,
        };

        const delayMs = Math.floor(500 + Math.random() * 10001); // 0..10000 ms
        sleep(delayMs / 1000);
        try {
          socket.send(JSON.stringify(vote));
          log(
            `VU ${__VU} sent vote after ${delayMs}ms: ${JSON.stringify(vote)}`,
            1,
          );
        } catch (e) {
          console.error(`VU ${__VU} failed to send vote:`, e);
        }

        // send vote back; use socket.send or socket.send(JSON.stringify(vote)) depending on server expectations
        // try {
        //   socket.send(JSON.stringify(vote));
        //   log(`VU ${__VU} sent vote: ${JSON.stringify(vote)}`, 1);
        // } catch (e) {
        //   console.error(`VU ${__VU} failed to send vote:`, e);
        // }
      }
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
