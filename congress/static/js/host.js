/**
 * Host panel main entry point
 * Coordinates all modules and handles WebSocket communication
 */

// Import from common
import {
  WSConnection,
  CountdownTimer,
  restorePanelFromUrl,
  showPanel,
  copyToClipboard,
  escapeHtml,
} from "./common.js";

// Import from host modules
import { gameState, resetRoundUiState } from "./host/state.js";
import {
  updateStatus,
  updateUI,
  updateCurrentRoundInfo,
  showAlert,
  log,
  clearLog,
  generateJoinQRCodes,
  copyAudienceUrl,
  updateScores,
} from "./host/ui.js";
import {
  getPlayerCount,
  updatePlayersList,
  removePlayer,
} from "./host/players.js";
import {
  addPrompt,
  addPromptFromOverview,
  setupImagePreview,
  selectPrompt,
  selectPromptById,
  togglePromptSection,
  filterPrompts,
  pickRandomPrompt,
  shadowbanPromptSubmitters,
  shadowbanAudience,
  updatePromptsList,
  updateQueuedPromptsList,
  queuePrompt,
  unqueuePrompt,
  deletePrompt,
  startPromptSelection,
} from "./host/prompts.js";
import {
  regenerateAi,
  writeManualAiSubmission,
  selectAiSubmission,
  removeSubmission,
  handleAiGenerationStatus,
} from "./host/ai-manager.js";
import {
  updateSubmissionsList,
  markDuplicate,
  setRevealOrder,
  setWsConn as setSubmissionsWsConn,
} from "./host/submissions.js";
import {
  runOverviewPrimaryAction,
  runOverviewSecondaryAction,
  updateOverviewFlow,
  updateOverviewRevealStatus,
  filterOverviewPrompts,
  updateOverviewPromptPool,
  maybeAutoQueueOverviewPrompt,
  setWsConn as setOverviewWsConn,
  setCallbacks as setOverviewCallbacks,
} from "./host/overview.js";
import {
  refreshStateView,
  downloadStateExport,
  copyStateToClipboard,
  handleStateFileSelect,
  validateStateImport,
  executeStateImport,
} from "./host/state-export.js";

// Module-level variables
let wsConn = null;
let hostTimer = null;

// Initialize
function init() {
  wsConn = new WSConnection("host", handleMessage, updateStatus);
  wsConn.connect();

  // Set wsConn on modules that need it
  setSubmissionsWsConn(wsConn);
  setOverviewWsConn(wsConn);

  // Set callbacks for overview module
  setOverviewCallbacks({
    hostCreatePlayersFromOverview,
    hostCreatePlayers,
    startPromptSelection: () => startPromptSelection(wsConn),
    transitionPhase,
    closeWriting,
    extendTimer,
    revealNext,
  });

  // Initialize timer
  hostTimer = new CountdownTimer("hostTimer");

  // Generate QR codes for joining
  generateJoinQRCodes();

  // Setup image preview for multimodal prompts
  setupImagePreview();

  // Restore panel from URL if present (allows reload to stay on same panel)
  restorePanelFromUrl();
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
        updateUI(uiCallbacks);
        updatePanicModeUI();
        // Start timer if deadline exists
        if (gameState.deadline && message.server_now) {
          hostTimer.start(gameState.deadline, message.server_now);
        }
      }
      break;

    case "phase":
      // Detect round change via phase broadcast (covers implicit round starts)
      if (
        typeof message.round_no === "number" &&
        message.round_no !== gameState.roundNo
      ) {
        resetRoundUiState(resetCallbacks);
      }
      gameState.phase = message.phase;
      gameState.roundNo = message.round_no;
      gameState.validTransitions = message.valid_transitions || [];
      gameState.deadline = message.deadline || null;
      if (message.prompt) {
        gameState.currentPrompt = message.prompt;
      }
      updateUI(uiCallbacks);
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

    case "reveal_update":
      if (gameState.currentRound) {
        gameState.currentRound.reveal_index = message.reveal_index;
      }
      updateOverviewRevealStatus();
      break;

    case "deadline_update":
      gameState.deadline = message.deadline;
      if (hostTimer && message.deadline && message.server_now) {
        hostTimer.updateDeadline(message.deadline, message.server_now);
      }
      showAlert("Timer verlngert!", "success");
      break;

    case "players_created":
      // Extract tokens from PlayerToken objects
      gameState.players = (message.players || []).map((p) => p.token);
      updatePlayersList();
      updateOverviewFlow();
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
      updateOverviewFlow();
      updateOverviewRevealStatus();
      break;

    case "host_player_status":
      gameState.playerStatus = message.players || [];
      updatePlayersList();
      updateOverviewFlow();
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
        gameState.currentRound = null;
        gameState.currentPrompt = null;
        updateUI(uiCallbacks);
        updatePlayersList();
        updateSubmissionsList();
        updateScores();
        showAlert("Spiel wurde zurckgesetzt", "success");
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
      gameState.promptStats = message.stats || {
        total: 0,
        host_count: 0,
        audience_count: 0,
        top_submitters: [],
      };
      updatePromptsList();
      updateOverviewPromptPool();
      maybeAutoQueueOverviewPrompt();
      break;

    case "host_queued_prompts":
      gameState.queuedPrompts = message.prompts || [];
      updateQueuedPromptsList();
      updatePromptsList(); // Update pool to show queue status
      updateOverviewPromptPool();
      maybeAutoQueueOverviewPrompt();
      updateOverviewFlow();
      break;

    case "round_started":
      gameState.currentRound = message.round;
      gameState.roundNo = message.round.number;
      gameState.currentPrompt = message.round.selected_prompt || null;
      resetRoundUiState(resetCallbacks);
      updateCurrentRoundInfo();
      updateUI(uiCallbacks);
      updateOverviewRevealStatus();
      log(`Runde ${message.round.number} gestartet`, "info");
      break;

    case "prompt_selected":
      gameState.currentPrompt = message.prompt;
      updateCurrentRoundInfo();
      showAlert("Prompt ausgewhlt - Runde wird vorbereitet", "success");
      break;

    case "player_removed":
      log(`Spieler ${message.player_id} entfernt`, "info");
      // Player status update will come separately via host_player_status
      break;

    case "error":
      showAlert(`Fehler: ${message.msg}`, "error");
      break;

    default:
      console.log("Unhandled message type:", message.t, message);
      break;
  }
}

// Callbacks for resetRoundUiState
const resetCallbacks = {
  updateSubmissionsList,
  updatePanicModeUI,
  updateOverviewFlow,
  updateOverviewRevealStatus,
};

// Callbacks for updateUI
const uiCallbacks = {
  getPlayerCount,
  updateOverviewFlow,
  updateOverviewRevealStatus,
};

// Host Commands
function transitionPhase(phase) {
  wsConn.send({ t: "host_transition_phase", phase: phase });
}

function hostCreatePlayers(count) {
  wsConn.send({ t: "host_create_players", count: count });
}

function hostCreatePlayersFromOverview() {
  const count = parseInt(
    document.getElementById("overviewPlayerCount")?.value ?? "0",
    10,
  );
  if (count > 0) {
    hostCreatePlayers(count);
  }
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

function revealNext() {
  wsConn.send({ t: "host_reveal_next" });
  log("Zur nchsten Antwort gewechselt", "info");
}

function revealPrev() {
  wsConn.send({ t: "host_reveal_prev" });
  log("Zur vorherigen Antwort gewechselt", "info");
}

function resetGame() {
  if (
    confirm(
      "Willst du das Spiel wirklich zurcksetzen? Das kann nicht rckgngig gemacht werden.",
    )
  ) {
    wsConn.send({ t: "host_reset_game" });
  }
}

function clearPromptPool() {
  if (
    confirm(
      "Willst du alle Prompts aus dem Pool lschen? Das kann nicht rckgngig gemacht werden.",
    )
  ) {
    wsConn.send({ t: "host_clear_prompt_pool" });
  }
}

function togglePanicMode() {
  const newState = !gameState.panicMode;
  if (
    newState &&
    !confirm(
      "PANIK-MODUS AKTIVIEREN?\n\nDas Publikum kann dann nicht mehr abstimmen. Du musst die Gewinner manuell auswhlen.",
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

function extendTimer(seconds) {
  if (!gameState.deadline) {
    showAlert("Kein aktiver Timer zum Verlngern", "error");
    return;
  }
  wsConn.send({
    t: "host_extend_timer",
    seconds: seconds,
  });
}

function closeWriting() {
  transitionPhase("REVEAL");
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
        '<p style="opacity: 0.6;">Antworten werden hier angezeigt, sobald verfgbar.</p>';
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

// Initialize on page load
init();

// Expose functions to window for onclick handlers in HTML
if (typeof window !== "undefined") {
  Object.assign(window, {
    // Panel navigation
    showPanel,

    // Phase transitions
    transitionPhase,

    // Player management
    hostCreatePlayers,
    hostCreatePlayersFromOverview,
    hostCreatePlayersCustom,
    removePlayer: (playerId, playerName) =>
      removePlayer(playerId, playerName, wsConn),

    // Round management
    hostStartRound,
    closeWriting,
    revealNext,
    revealPrev,

    // Prompts
    addPrompt: () => addPrompt(wsConn),
    addPromptFromOverview: (queue) => addPromptFromOverview(queue, wsConn),
    selectPrompt: () => selectPrompt(wsConn),
    selectPromptById: (id) => selectPromptById(id, wsConn),
    queuePrompt: (id) => queuePrompt(id, wsConn),
    unqueuePrompt: (id) => unqueuePrompt(id, wsConn),
    deletePrompt: (id) => deletePrompt(id, wsConn),
    startPromptSelection: () => startPromptSelection(wsConn),
    togglePromptSection,
    filterPrompts,
    pickRandomPrompt: () => pickRandomPrompt(wsConn),
    shadowbanAudience: (id) => shadowbanAudience(id, wsConn),
    shadowbanPromptSubmitters: (id) => shadowbanPromptSubmitters(id, wsConn),

    // AI management
    regenerateAi: () => regenerateAi(wsConn),
    writeManualAiSubmission: () => writeManualAiSubmission(wsConn),
    selectAiSubmission: (id) => selectAiSubmission(id, wsConn),
    removeSubmission: (id) => removeSubmission(id, wsConn),

    // Submissions
    markDuplicate: (id) => markDuplicate(id, wsConn),
    setRevealOrder: () => setRevealOrder(wsConn),

    // Game control
    resetGame,
    clearPromptPool,
    togglePanicMode,
    setManualWinner,
    extendTimer,

    // Overview
    runOverviewPrimaryAction,
    runOverviewSecondaryAction,
    filterOverviewPrompts,

    // State export/import
    refreshStateView,
    downloadStateExport,
    copyStateToClipboard,
    handleStateFileSelect,
    validateStateImport,
    executeStateImport,

    // Utilities
    clearLog,
    copyAudienceUrl,
    copyToClipboard,
  });
}
