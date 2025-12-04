/**
 * Host-specific JavaScript
 */

let wsConn = null;
let hostTimer = null;
const gameState = {
  phase: "LOBBY",
  roundNo: 0,
  players: [], // Legacy: just tokens
  playerStatus: [], // New: full player status with names
  submissions: [],
  prompts: [], // Prompt candidates from audience (filtered by shadowban)
  scores: { players: [], audience_top: [] },
  validTransitions: [], // Populated by server
  panicMode: false,
  deadline: null,
  selectedAiSubmissionId: null, // Currently selected AI submission
  aiGenerationStatus: "idle", // idle, generating, completed, failed
};

// Initialize
function init() {
  wsConn = new WSConnection("host", handleMessage, updateStatus);
  wsConn.connect();

  // Initialize timer
  hostTimer = new CountdownTimer("hostTimer");

  // Generate QR codes for joining
  generateJoinQRCodes();
}

function handleMessage(message) {
  switch (message.t) {
    case "welcome":
      if (message.game) {
        gameState.phase = message.game.phase;
        gameState.roundNo = message.game.round_no;
        gameState.validTransitions = message.valid_transitions || [];
        gameState.panicMode = message.game.panic_mode || false;
        gameState.deadline = message.game.phase_deadline || null;
        updateUI();
        updatePanicModeUI();
        // Start timer if deadline exists
        if (gameState.deadline && message.server_now) {
          hostTimer.start(gameState.deadline, message.server_now);
        }
      }
      break;

    case "phase":
      gameState.phase = message.phase;
      gameState.validTransitions = message.valid_transitions || [];
      gameState.deadline = message.deadline || null;
      updateUI();
      // Update timer
      if (gameState.deadline && message.server_now) {
        hostTimer.start(gameState.deadline, message.server_now);
      } else {
        hostTimer.stop();
        hostTimer.hide();
        document.getElementById("hostTimer").textContent = "--:--";
      }
      showAlert(`Phase gewechselt zu: ${message.phase}`, "success");
      break;

    case "deadline_update":
      gameState.deadline = message.deadline;
      if (hostTimer && message.deadline && message.server_now) {
        hostTimer.updateDeadline(message.deadline, message.server_now);
      }
      showAlert("Timer verlängert!", "success");
      break;

    case "players_created":
      // Extract tokens from PlayerToken objects
      gameState.players = (message.players || []).map((p) => p.token);
      updatePlayersList();
      showAlert(`${gameState.players.length} Spieler erstellt`, "success");
      break;

    case "submissions":
      // Host should ignore public submissions and use host_submissions instead
      // (host_submissions includes author_kind which we need for managing the game)
      break;

    case "host_submissions":
      gameState.submissions = message.list || [];
      updateSubmissionsList();
      updatePanicModeUI();
      break;

    case "host_player_status":
      gameState.playerStatus = message.players || [];
      updatePlayersList();
      break;

    case "scores":
      gameState.scores = {
        players: message.players || [],
        audience_top: message.audience_top || [],
      };
      updateScores();
      break;

    case "game_state":
      if (message.game) {
        gameState.phase = message.game.phase;
        gameState.roundNo = message.game.round_no;
        gameState.validTransitions = message.valid_transitions || [];
        gameState.players = [];
        gameState.submissions = [];
        gameState.scores = { players: [], audience_top: [] };
        updateUI();
        updatePlayersList();
        updateSubmissionsList();
        updateScores();
        showAlert("Spiel wurde zurückgesetzt", "success");
      }
      break;

    case "panic_mode_update":
      gameState.panicMode = message.enabled;
      updatePanicModeUI();
      showAlert(
        message.enabled ? "PANIK-MODUS AKTIVIERT" : "Panik-Modus deaktiviert",
        message.enabled ? "error" : "success",
      );
      break;

    case "ai_generation_status":
      handleAiGenerationStatus(message);
      break;

    case "host_prompts":
      gameState.prompts = message.prompts || [];
      updatePromptsList();
      break;

    case "error":
      showAlert(`Fehler: ${message.msg}`, "error");
      break;

    default:
      console.log("Unhandled message type:", message.t, message);
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

  // Update phase transition buttons
  updatePhaseButtons();
}

/**
 * Update phase transition buttons based on valid transitions from server
 */
function updatePhaseButtons() {
  const currentPhase = gameState.phase;
  const validTargets = gameState.validTransitions || [];
  const container = document.getElementById("phaseButtons");

  if (!container) return;

  const buttons = container.querySelectorAll("button[data-phase]");
  buttons.forEach((btn) => {
    const targetPhase = btn.dataset.phase;
    const isValid = validTargets.includes(targetPhase);
    const isCurrent = targetPhase === currentPhase;

    btn.disabled = !isValid || isCurrent;

    // Add visual indicator for current phase
    btn.classList.toggle("current", isCurrent);
  });
}

// Host Commands
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
    alert("Bitte gib einen Prompt ein");
    return;
  }

  wsConn.send({
    t: "host_add_prompt",
    text: text,
  });

  document.getElementById("promptText").value = "";
}

function selectPrompt() {
  const promptId = prompt("Gib die Prompt-ID ein:");
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

function togglePanicMode() {
  const newState = !gameState.panicMode;
  if (
    newState &&
    !confirm(
      "PANIK-MODUS AKTIVIEREN?\n\nDas Publikum kann dann nicht mehr abstimmen. Du musst die Gewinner manuell auswählen.",
    )
  ) {
    return;
  }
  wsConn.send({ t: "host_toggle_panic_mode", enabled: newState });
}

function setManualWinner(winnerType, submissionId) {
  wsConn.send({
    t: "host_set_manual_winner",
    winner_type: winnerType,
    submission_id: submissionId,
  });
  showAlert(
    `Manueller ${winnerType === "ai" ? "KI" : "Lustigster"}-Gewinner gesetzt`,
    "success",
  );
}

function markDuplicate(submissionId) {
  if (
    confirm(
      "Diese Antwort als Duplikat markieren?\n\nDer Spieler wird benachrichtigt und muss eine neue Antwort einreichen.",
    )
  ) {
    wsConn.send({
      t: "host_mark_duplicate",
      submission_id: submissionId,
    });
    showAlert("Antwort als Duplikat markiert", "success");
  }
}

function extendTimer(seconds) {
  if (!gameState.deadline) {
    showAlert("Kein aktiver Timer zum Verlängern", "error");
    return;
  }
  wsConn.send({
    t: "host_extend_timer",
    seconds: seconds,
  });
}

// AI Management Functions
function regenerateAi() {
  wsConn.send({ t: "host_regenerate_ai" });
  showAlert("KI-Generierung gestartet...", "info");
}

function writeManualAiSubmission() {
  const text = document.getElementById("manualAiText").value.trim();
  if (!text) {
    showAlert("Bitte gib einen Text für die KI-Antwort ein", "error");
    return;
  }
  wsConn.send({
    t: "host_write_ai_submission",
    text: text,
  });
  document.getElementById("manualAiText").value = "";
  showAlert("Manuelle KI-Antwort gespeichert", "success");
}

function selectAiSubmission(submissionId) {
  wsConn.send({
    t: "host_set_ai_submission",
    submission_id: submissionId,
  });
  gameState.selectedAiSubmissionId = submissionId;
  updateAiSubmissionsList();
  showAlert("KI-Antwort ausgewählt", "success");
}

function handleAiGenerationStatus(message) {
  gameState.aiGenerationStatus = message.status;
  updateAiGenerationStatusUI(message);

  switch (message.status) {
    case "started":
      showAlert("KI-Generierung gestartet...", "info");
      break;
    case "completed":
      showAlert("KI-Generierung abgeschlossen!", "success");
      break;
    case "all_failed":
      showAlert(
        `KI-Generierung fehlgeschlagen: ${message.message || "Unbekannter Fehler"}`,
        "error",
      );
      break;
  }
}

function updateAiGenerationStatusUI(message) {
  const statusEl = document.getElementById("aiGenerationStatus");
  if (!statusEl) return;

  let statusText = "";
  let statusClass = "";

  switch (message?.status || gameState.aiGenerationStatus) {
    case "started":
      statusText = "KI-Status: Generiere Antworten...";
      statusClass = "info";
      break;
    case "completed":
      statusText = "KI-Status: Generierung abgeschlossen";
      statusClass = "success";
      break;
    case "all_failed":
      statusText = `KI-Status: Fehlgeschlagen - ${message?.message || "Alle Provider haben keine Antwort geliefert"}`;
      statusClass = "error";
      break;
    default:
      statusText = "KI-Status: Warte auf Prompt-Auswahl";
      statusClass = "";
  }

  statusEl.innerHTML = `<p class="${statusClass}" style="margin: 0;">${statusText}</p>`;
}

function updateAiSubmissionsList() {
  const container = document.getElementById("aiSubmissionsList");
  if (!container) return;

  const aiSubmissions = gameState.submissions.filter(
    (s) => s.author_kind === "ai",
  );

  if (aiSubmissions.length === 0) {
    container.innerHTML =
      '<p style="opacity: 0.6;">Keine KI-Antworten vorhanden</p>';
    return;
  }

  let html =
    '<p style="margin-bottom: 10px; opacity: 0.8;">Wähle die KI-Antwort für diese Runde:</p>';
  html += '<div class="ai-submissions-grid">';

  aiSubmissions.forEach((sub) => {
    const isSelected = gameState.selectedAiSubmissionId === sub.id;
    const provider = sub.author_ref || "unbekannt";
    // Parse provider info (format: "provider:model")
    const providerParts = provider.split(":");
    const providerName = providerParts[0] || "?";
    const modelName = providerParts[1] || "";

    html += `
      <div class="ai-submission-card ${isSelected ? "selected" : ""}" onclick="selectAiSubmission('${sub.id}')">
        <div class="ai-card-header">
          <span class="provider-badge">${escapeHtml(providerName)}</span>
          ${modelName ? `<span class="model-name">${escapeHtml(modelName)}</span>` : ""}
          ${isSelected ? '<span class="selected-badge">AUSGEWÄHLT</span>' : ""}
        </div>
        <div class="ai-card-text">${escapeHtml(sub.display_text)}</div>
        <div class="ai-card-actions">
          <button class="${isSelected ? "" : "secondary"}" onclick="event.stopPropagation(); selectAiSubmission('${sub.id}')">
            ${isSelected ? "Ausgewählt" : "Auswählen"}
          </button>
        </div>
      </div>
    `;
  });

  html += "</div>";
  container.innerHTML = html;
}

function updatePanicModeUI() {
  const panicBtn = document.getElementById("panicModeBtn");
  const panicStatus = document.getElementById("panicStatus");
  const manualWinnerSection = document.getElementById("manualWinnerSection");
  const manualWinnerButtons = document.getElementById("manualWinnerButtons");

  if (panicBtn) {
    panicBtn.textContent = gameState.panicMode
      ? "Panik-Modus DEAKTIVIEREN"
      : "PANIK-MODUS aktivieren";
    panicBtn.classList.toggle("active", gameState.panicMode);
  }

  if (panicStatus) {
    panicStatus.textContent = gameState.panicMode ? "AKTIV" : "Inaktiv";
    panicStatus.classList.toggle("active", gameState.panicMode);
  }

  if (manualWinnerSection) {
    manualWinnerSection.style.display = gameState.panicMode ? "block" : "none";
  }

  // Populate manual winner buttons
  if (manualWinnerButtons && gameState.panicMode) {
    if (gameState.submissions.length === 0) {
      manualWinnerButtons.innerHTML =
        '<p style="opacity: 0.6;">Antworten werden hier angezeigt, sobald verfügbar.</p>';
    } else {
      let html =
        '<div style="margin-bottom: 15px;"><strong>Als KI-Gewinner markieren:</strong></div>';
      html += '<div class="button-group" style="margin-bottom: 20px;">';
      gameState.submissions.forEach((sub, idx) => {
        const shortText =
          sub.display_text.substring(0, 30) +
          (sub.display_text.length > 30 ? "..." : "");
        html += `<button class="secondary" onclick="setManualWinner('ai', '${sub.id}')" title="${escapeHtml(sub.display_text)}">${idx + 1}. ${escapeHtml(shortText)}</button>`;
      });
      html += "</div>";

      html +=
        '<div style="margin-bottom: 15px;"><strong>Als Lustigster markieren:</strong></div>';
      html += '<div class="button-group">';
      gameState.submissions.forEach((sub, idx) => {
        const shortText =
          sub.display_text.substring(0, 30) +
          (sub.display_text.length > 30 ? "..." : "");
        html += `<button class="secondary" onclick="setManualWinner('funny', '${sub.id}')" title="${escapeHtml(sub.display_text)}">${idx + 1}. ${escapeHtml(shortText)}</button>`;
      });
      html += "</div>";

      manualWinnerButtons.innerHTML = html;
    }
  }
}

function closeWriting() {
  transitionPhase("REVEAL");
}

// UI Updates
function updatePlayersList() {
  const container = document.getElementById("playerTokensList");
  container.innerHTML = "";

  // Use playerStatus if available, fall back to legacy players array
  const players =
    gameState.playerStatus.length > 0
      ? gameState.playerStatus
      : gameState.players.map((token, idx) => ({
          token,
          display_name: null,
          status: "not_submitted",
          id: `player_${idx}`,
        }));

  if (players.length === 0) {
    container.innerHTML = '<p style="opacity: 0.6;">Keine Spieler erstellt</p>';
    return;
  }

  players.forEach((player) => {
    const div = document.createElement("div");
    div.className = "player-status-card";

    // Determine status display
    const token = typeof player === "string" ? player : player.token;
    const name = player.display_name || "Nicht registriert";
    const status = player.status || "not_submitted";

    // Status badge
    let statusBadge = "";
    let statusClass = "";
    switch (status) {
      case "submitted":
        statusBadge = "✅ Eingereicht";
        statusClass = "submitted";
        break;
      case "checking_typos":
        statusBadge = "🔄 Prüft...";
        statusClass = "checking";
        break;
      default:
        statusBadge = "⏳ Wartet";
        statusClass = "waiting";
    }

    div.innerHTML = `
      <div class="player-info">
        <div class="player-header">
          <span class="player-name">${escapeHtml(name)}</span>
          <span class="status-badge ${statusClass}">${statusBadge}</span>
        </div>
        <div class="player-token">
          <span class="token">${token}</span>
          <button onclick="copyToClipboard('${token}')" class="copy-btn">📋</button>
        </div>
      </div>
    `;
    container.appendChild(div);
  });

  // Update overview count
  const overviewPlayers = document.getElementById("overviewPlayers");
  if (overviewPlayers) {
    overviewPlayers.textContent = players.length;
  }
}

function updatePromptsList() {
  const container = document.getElementById("promptsList");
  if (!container) return;
  container.innerHTML = "";

  if (gameState.prompts.length === 0) {
    container.innerHTML =
      '<p style="opacity: 0.6;">Keine Prompts von Publikum verfügbar</p>';
    return;
  }

  gameState.prompts.forEach((prompt) => {
    const div = document.createElement("div");
    div.className = "prompt-card";

    const isAudience = prompt.source === "audience";
    const shortId = prompt.submitter_id
      ? `${prompt.submitter_id.substring(0, 8)}...`
      : "";

    div.innerHTML = `
      <div class="prompt-header">
        <span class="prompt-id">${prompt.id.substring(0, 12)}...</span>
        <span class="badge ${isAudience ? "audience" : "host"}">${isAudience ? "Publikum" : "Host"}</span>
        ${shortId ? `<span class="submitter-id" title="${escapeHtml(prompt.submitter_id)}">${shortId}</span>` : ""}
      </div>
      <div class="prompt-text">${escapeHtml(prompt.text || "(Kein Text)")}</div>
      <div class="prompt-actions">
        <button class="secondary" onclick="selectPromptById('${prompt.id}')">Auswählen</button>
        ${isAudience && prompt.submitter_id ? `<button class="danger" onclick="shadowbanAudience('${prompt.submitter_id}')" title="Diesen Nutzer shadowbannen (ignoriert zukünftige Prompts)">🚫 Shadowban</button>` : ""}
      </div>
    `;
    container.appendChild(div);
  });
}

function selectPromptById(promptId) {
  wsConn.send({
    t: "host_select_prompt",
    prompt_id: promptId,
  });
}

function shadowbanAudience(voterId) {
  if (
    confirm(
      "Diesen Nutzer shadowbannen?\n\nAlle zukünftigen Prompts von diesem Nutzer werden ignoriert. Der Nutzer erfährt davon nichts.",
    )
  ) {
    wsConn.send({
      t: "host_shadowban_audience",
      voter_id: voterId,
    });
    showAlert("Nutzer shadowbanned", "success");
  }
}

function updateSubmissionsList() {
  const container = document.getElementById("submissionsList");
  if (!container) return;
  container.innerHTML = "";

  // Also update the AI-specific submissions list
  updateAiSubmissionsList();

  if (gameState.submissions.length === 0) {
    container.innerHTML = '<p style="opacity: 0.6;">Noch keine Antworten</p>';
    return;
  }

  gameState.submissions.forEach((sub) => {
    const div = document.createElement("div");
    const authorKind = sub.author_kind || "unknown";
    const isSelectedAi =
      authorKind === "ai" && gameState.selectedAiSubmissionId === sub.id;
    div.className = `submission-card${authorKind === "ai" ? " ai" : ""}${isSelectedAi ? " selected-ai" : ""}`;

    // Show provider info for AI submissions
    const providerInfo =
      authorKind === "ai" && sub.author_ref
        ? `<span class="provider-info">(${escapeHtml(sub.author_ref)})</span>`
        : "";

    div.innerHTML = `
            <div class="header">
                <span>${sub.id}</span>
                <span class="badge ${authorKind}">${authorKind.toUpperCase()}</span>
                ${providerInfo}
                ${isSelectedAi ? '<span class="badge selected">AUSGEWÄHLT</span>' : ""}
            </div>
            <div class="text">${escapeHtml(sub.display_text)}</div>
            <div class="actions">
                ${authorKind === "player" ? `<button onclick="setAiSubmission('${sub.id}')">Als KI markieren</button>` : ""}
                ${authorKind === "ai" && !isSelectedAi ? `<button onclick="selectAiSubmission('${sub.id}')">Als KI auswählen</button>` : ""}
                ${authorKind === "player" ? `<button class="danger" onclick="markDuplicate('${sub.id}')">Dupe</button>` : ""}
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

// ============================================================================
// State Export/Import Functions
// ============================================================================

let cachedStateExport = null;

/**
 * Fetch and display the current state
 */
async function refreshStateView() {
  const viewer = document.getElementById("stateJsonView");
  viewer.textContent = "Lade...";

  try {
    const response = await fetch("/api/state/export");
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }
    cachedStateExport = await response.json();
    viewer.textContent = JSON.stringify(cachedStateExport, null, 2);
    showAlert("State geladen", "success");
  } catch (error) {
    viewer.textContent = `Fehler: ${error.message}`;
    showAlert(`Fehler beim Laden: ${error.message}`, "error");
  }
}

/**
 * Download the state as a JSON file
 */
async function downloadStateExport() {
  try {
    // Always fetch fresh data for download
    const response = await fetch("/api/state/export");
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }
    const data = await response.json();

    const blob = new Blob([JSON.stringify(data, null, 2)], {
      type: "application/json",
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
    a.download = `gptdash-state-${timestamp}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);

    showAlert("State-Datei heruntergeladen", "success");
  } catch (error) {
    showAlert(`Fehler beim Download: ${error.message}`, "error");
  }
}

/**
 * Copy current state to clipboard
 */
async function copyStateToClipboard() {
  try {
    const response = await fetch("/api/state/export");
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }
    const data = await response.json();
    await navigator.clipboard.writeText(JSON.stringify(data, null, 2));
    showAlert("State in Zwischenablage kopiert", "success");
  } catch (error) {
    showAlert(`Fehler: ${error.message}`, "error");
  }
}

/**
 * Handle file selection for import
 */
function handleStateFileSelect(event) {
  const file = event.target.files[0];
  if (!file) return;

  const reader = new FileReader();
  reader.onload = (e) => {
    document.getElementById("stateImportText").value = e.target.result;
    showImportStatus("Datei geladen. Klicke 'Validieren' zur Prüfung.", "info");
  };
  reader.onerror = () => {
    showImportStatus("Fehler beim Lesen der Datei", "error");
  };
  reader.readAsText(file);
}

/**
 * Validate the import JSON without importing
 */
function validateStateImport() {
  const text = document.getElementById("stateImportText").value.trim();
  if (!text) {
    showImportStatus("Bitte JSON eingeben oder Datei hochladen", "error");
    return null;
  }

  try {
    const data = JSON.parse(text);

    // Basic validation
    if (!data.schema_version) {
      showImportStatus("Fehler: schema_version fehlt", "error");
      return null;
    }

    // Count objects for summary
    const summary = [];
    if (data.game) summary.push("1 Game");
    if (data.rounds) summary.push(`${Object.keys(data.rounds).length} Runden`);
    if (data.players)
      summary.push(`${Object.keys(data.players).length} Spieler`);
    if (data.submissions)
      summary.push(`${Object.keys(data.submissions).length} Antworten`);
    if (data.votes) summary.push(`${Object.keys(data.votes).length} Votes`);
    if (data.scores) summary.push(`${data.scores.length} Scores`);

    showImportStatus(
      `✓ JSON valide (Schema v${data.schema_version}): ${summary.join(", ")}`,
      "success",
    );
    return data;
  } catch (error) {
    showImportStatus(`JSON-Parsing Fehler: ${error.message}`, "error");
    return null;
  }
}

/**
 * Execute the state import
 */
async function executeStateImport() {
  const data = validateStateImport();
  if (!data) return;

  if (
    !confirm(
      "ACHTUNG: Der gesamte Spielzustand wird ersetzt!\n\n" +
        "Alle verbundenen Clients werden über den neuen Zustand informiert.\n\n" +
        "Fortfahren?",
    )
  ) {
    return;
  }

  showImportStatus("Importiere...", "info");

  try {
    const response = await fetch("/api/state/import", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(data),
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(errorText);
    }

    showImportStatus("✓ State erfolgreich importiert!", "success");
    showAlert("State importiert! UI wird aktualisiert...", "success");

    // Clear the import textarea
    document.getElementById("stateImportText").value = "";
    document.getElementById("stateImportFile").value = "";

    // Refresh the state view
    await refreshStateView();
  } catch (error) {
    showImportStatus(`Import fehlgeschlagen: ${error.message}`, "error");
    showAlert(`Import fehlgeschlagen: ${error.message}`, "error");
  }
}

/**
 * Show status message in import section
 */
function showImportStatus(message, type) {
  const el = document.getElementById("stateImportStatus");
  el.innerHTML = `<div class="alert ${type}" style="margin: 0;">${message}</div>`;
}

// Initialize on page load
init();

if (typeof window !== "undefined") {
  Object.assign(window, {
    transitionPhase,
    hostCreatePlayers,
    hostCreatePlayersCustom,
    hostStartRound,
    addPrompt,
    selectPrompt,
    selectPromptById,
    setAiSubmission,
    setRevealOrder,
    resetGame,
    closeWriting,
    clearLog,
    copyAudienceUrl,
    togglePanicMode,
    setManualWinner,
    markDuplicate,
    extendTimer,
    regenerateAi,
    writeManualAiSubmission,
    selectAiSubmission,
    shadowbanAudience,
    // State export/import
    refreshStateView,
    downloadStateExport,
    copyStateToClipboard,
    handleStateFileSelect,
    validateStateImport,
    executeStateImport,
  });
}
