/**
 * Host-specific JavaScript
 */

let wsConn = null;
const gameState = {
  phase: "LOBBY",
  roundNo: 0,
  players: [],
  submissions: [],
  scores: { players: [], audience_top: [] },
};

// Initialize
function init() {
  wsConn = new WSConnection("host", handleMessage, updateStatus);
  wsConn.connect();

  // Generate QR codes for joining
  generateJoinQRCodes();
}

function handleMessage(message) {
  switch (message.t) {
    case "welcome":
      if (message.game) {
        gameState.phase = message.game.phase;
        gameState.roundNo = message.game.round_no;
        updateUI();
      }
      break;

    case "phase":
      gameState.phase = message.phase;
      updateUI();
      showAlert(`Phase gewechselt zu: ${message.phase}`, "success");
      break;

    case "players_created":
      gameState.players = message.tokens || [];
      updatePlayersList();
      showAlert(`${message.tokens.length} Spieler erstellt`, "success");
      break;

    case "submissions":
      gameState.submissions = message.list || [];
      updateSubmissionsList();
      break;

    case "scores":
      gameState.scores = {
        players: message.players || [],
        audience_top: message.audience_top || [],
      };
      updateScores();
      break;

    case "error":
      showAlert(`Fehler: ${message.msg}`, "error");
      break;
  }
}

function updateStatus(connected) {
  const dot = document.getElementById("statusDot");
  const text = document.getElementById("statusText");

  if (connected) {
    dot.classList.add("connected");
    text.textContent = "Verbunden";
    log("Mit Spiel-Server verbunden", "info");
  } else {
    dot.classList.remove("connected");
    text.textContent = "Nicht verbunden";
    log("Verbindung getrennt", "info");
  }
}

function updateUI() {
  // Update header displays
  document.getElementById("phaseDisplay").textContent =
    `Phase: ${gameState.phase}`;
  document.getElementById("roundDisplay").textContent =
    `Round: ${gameState.roundNo}`;

  // Update overview
  document.getElementById("overviewPhase").textContent = gameState.phase;
  document.getElementById("overviewRound").textContent = gameState.roundNo;
  document.getElementById("overviewPlayers").textContent =
    gameState.players.length;
  document.getElementById("overviewSubmissions").textContent =
    gameState.submissions.length;
}

// Host Commands
function createGame() {
  wsConn.send({ t: "host_create_game" });
}

function transitionPhase(phase) {
  wsConn.send({ t: "host_transition_phase", phase: phase });
}

function hostCreatePlayers(count) {
  wsConn.send({ t: "host_create_players", count: count });
}

function hostCreatePlayersCustom() {
  const count = parseInt(document.getElementById("playerCount").value, 10);
  if (count > 0) {
    hostCreatePlayers(count);
  }
}

function hostStartRound() {
  wsConn.send({ t: "host_start_round" });
}

function addPrompt() {
  const text = document.getElementById("promptText").value.trim();
  if (!text) {
    alert("Bitte gib eine Frage ein");
    return;
  }

  wsConn.send({
    t: "host_add_prompt",
    text: text,
  });

  document.getElementById("promptText").value = "";
}

function selectPrompt() {
  const promptId = prompt("Gib die Fragen-ID ein:");
  if (promptId) {
    wsConn.send({
      t: "host_select_prompt",
      prompt_id: promptId,
    });
  }
}

function setAiSubmission(submissionId) {
  wsConn.send({
    t: "host_set_ai_submission",
    submission_id: submissionId,
  });
}

function setRevealOrder() {
  const input = document.getElementById("revealOrderInput").value.trim();
  if (!input) {
    alert("Bitte gib Antwort-IDs ein");
    return;
  }

  const order = input.split(",").map((s) => s.trim());

  wsConn.send({
    t: "host_set_reveal_order",
    order: order,
  });
}

// biome-ignore lint/correctness/noUnusedVariables: Called from HTML onclick
function revealNext() {
  wsConn.send({ t: "host_reveal_next" });
  log("Zur nächsten Antwort gewechselt", "info");
}

// biome-ignore lint/correctness/noUnusedVariables: Called from HTML onclick
function revealPrev() {
  wsConn.send({ t: "host_reveal_prev" });
  log("Zur vorherigen Antwort gewechselt", "info");
}

function resetGame() {
  if (
    confirm(
      "Willst du das Spiel wirklich zurücksetzen? Das kann nicht rückgängig gemacht werden.",
    )
  ) {
    wsConn.send({ t: "host_reset_game" });
  }
}

function closeWriting() {
  transitionPhase("REVEAL");
}

// UI Updates
function updatePlayersList() {
  const container = document.getElementById("playerTokensList");
  container.innerHTML = "";

  gameState.players.forEach((token, idx) => {
    const div = document.createElement("div");
    div.className = "token-display";
    div.innerHTML = `
            <span>Spieler ${idx + 1}: <span class="token">${token}</span></span>
            <button onclick="copyToClipboard('${token}')">Kopieren</button>
        `;
    container.appendChild(div);
  });
}

function updateSubmissionsList() {
  const container = document.getElementById("submissionsList");
  container.innerHTML = "";

  if (gameState.submissions.length === 0) {
    container.innerHTML = '<p style="opacity: 0.6;">Noch keine Antworten</p>';
    return;
  }

  gameState.submissions.forEach((sub) => {
    const div = document.createElement("div");
    div.className = `submission-card${sub.author_kind === "ai" ? " ai" : ""}`;
    div.innerHTML = `
            <div class="header">
                <span>${sub.id}</span>
                <span class="badge ${sub.author_kind}">${sub.author_kind.toUpperCase()}</span>
            </div>
            <div class="text">${escapeHtml(sub.display_text)}</div>
            <div class="actions">
                ${sub.author_kind === "player" ? `<button onclick="setAiSubmission('${sub.id}')">Als KI markieren</button>` : ""}
                <button class="secondary">Bearbeiten</button>
            </div>
        `;
    container.appendChild(div);
  });
}

function updateScores() {
  // Player scores
  const playerContainer = document.getElementById("playerScores");
  playerContainer.innerHTML = "";

  if (gameState.scores.players.length === 0) {
    playerContainer.innerHTML =
      '<p style="opacity: 0.6;">Noch keine Punkte</p>';
  } else {
    gameState.scores.players.forEach((score, idx) => {
      playerContainer.innerHTML += `
                <div class="info-item">
                    <div class="label">${idx + 1}. ${score.ref_id.substring(0, 12)}</div>
                    <div class="value">${score.total} Pkt</div>
                </div>
            `;
    });
  }

  // Audience scores
  const audienceContainer = document.getElementById("audienceScores");
  audienceContainer.innerHTML = "";

  if (gameState.scores.audience_top.length === 0) {
    audienceContainer.innerHTML =
      '<p style="opacity: 0.6;">Noch keine Publikums-Punkte</p>';
  } else {
    gameState.scores.audience_top.slice(0, 10).forEach((score, idx) => {
      audienceContainer.innerHTML += `
                <div class="info-item">
                    <div class="label">${idx + 1}. ${score.ref_id.substring(0, 12)}</div>
                    <div class="value">${score.total} Pkt</div>
                </div>
            `;
    });
  }
}

function showAlert(message, type = "info") {
  const container = document.getElementById("overviewAlert");
  container.innerHTML = `<div class="alert ${type}">${message}</div>`;

  // Auto-hide after 5 seconds
  setTimeout(() => {
    container.innerHTML = "";
  }, 5000);
}

function log(message, type = "info") {
  const logDiv = document.getElementById("messageLog");
  const entry = document.createElement("div");
  entry.className = `log-entry ${type}`;

  const timestamp = formatTime();
  entry.innerHTML = `<span class="timestamp">[${timestamp}]</span> ${message}`;

  logDiv.appendChild(entry);
  logDiv.scrollTop = logDiv.scrollHeight;
}

function clearLog() {
  document.getElementById("messageLog").innerHTML = "";
}

/**
 * Generate QR codes for joining the game
 */
function generateJoinQRCodes() {
  // Generate QR code for audience join
  const audienceUrl = QRCodeManager.generateAudienceQR("audienceQRCode", {
    width: 200,
    height: 200,
  });

  // Update URL display
  const urlEl = document.getElementById("audienceJoinUrl");
  if (urlEl) {
    urlEl.textContent = audienceUrl;
  }
}

/**
 * Copy audience join URL to clipboard
 */
function copyAudienceUrl() {
  const url = QRCodeManager.getAudienceJoinUrl();
  copyToClipboard(url);
  showAlert("URL in Zwischenablage kopiert!", "success");
}

// Initialize on page load
init();

if (typeof window !== "undefined") {
  Object.assign(window, {
    createGame,
    transitionPhase,
    hostCreatePlayers,
    hostCreatePlayersCustom,
    hostStartRound,
    addPrompt,
    selectPrompt,
    setAiSubmission,
    setRevealOrder,
    resetGame,
    closeWriting,
    clearLog,
    copyAudienceUrl,
  });
}
