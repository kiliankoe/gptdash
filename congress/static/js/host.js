/**
 * Host panel main entry point
 * Coordinates all modules and handles WebSocket communication
 */

// Import from common
import {
  copyToClipboard,
  CountdownTimer,
  escapeHtml,
  restorePanelFromUrl,
  showPanel,
  TTSManager,
  WSConnection,
} from "./common.js";

// Import from host modules
import {
  fetchAvailableModels,
  getSelectedModel,
  handleAiGenerationStatus,
  regenerateAi,
  removeSubmission,
  selectAiSubmission,
  setSelectedModel,
  writeManualAiSubmission,
} from "./host/ai-manager.js";
import {
  filterOverviewPrompts,
  maybeAutoQueueOverviewPrompt,
  runOverviewPrimaryAction,
  runOverviewSecondaryAction,
  setCallbacks as setOverviewCallbacks,
  setWsConn as setOverviewWsConn,
  updateOverviewFlow,
  updateOverviewPromptPool,
  updateOverviewRevealStatus,
} from "./host/overview.js";
import {
  getPlayerCount,
  removePlayer,
  updatePlayersList,
} from "./host/players.js";
import {
  addPrompt,
  addPromptFromOverview,
  deletePrompt,
  filterPrompts,
  pickRandomPrompt,
  queuePrompt,
  selectPrompt,
  selectPromptById,
  setupImagePreview,
  shadowbanAudience,
  shadowbanPromptSubmitters,
  startPromptSelection,
  togglePromptSection,
  unqueuePrompt,
  updatePromptsList,
  updateQueuedPromptsList,
} from "./host/prompts.js";
import {
  copyStateToClipboard,
  downloadStateExport,
  executeStateImport,
  handleStateFileSelect,
  refreshStateView,
  validateStateImport,
} from "./host/state-export.js";
import { gameState, resetRoundUiState } from "./host/state.js";
import {
  editSubmission,
  markDuplicate,
  setRevealOrder,
  setWsConn as setSubmissionsWsConn,
  updateSubmissionsList,
} from "./host/submissions.js";
import {
  copyPlayerUrl,
  generateJoinQRCodes,
  showAlert,
  updateCurrentRoundInfo,
  updateScores,
  updateStatus,
  updateUI,
} from "./host/ui.js";

// Module-level variables
let wsConn = null;
let hostTimer = null;
let tts = null;

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

  // Initialize TTS and check audio access
  tts = new TTSManager();
  checkAudioAccess();
  setupAudioUnlockButton();

  // Generate QR codes for joining
  generateJoinQRCodes();

  // Initialize trivia choice inputs (start with 2)
  renderTriviaChoices();

  // Setup image preview for multimodal prompts
  setupImagePreview();

  // Setup event delegation for data-action attributes (XSS prevention)
  setupEventDelegation();

  // Restore panel from URL if present (allows reload to stay on same panel)
  restorePanelFromUrl();

  // Fetch available AI models
  fetchAvailableModels();
}

function handleMessage(message) {
  switch (message.t) {
    case "welcome":
      if (message.game) {
        gameState.phase = message.game.phase;
        gameState.roundNo = message.game.round_no;
        gameState.validTransitions = message.valid_transitions || [];
        gameState.panicMode = message.game.panic_mode || false;
        gameState.softPanicMode = message.game.soft_panic_mode || false;
        gameState.venueOnlyMode = message.game.venue_only_mode || false;
        gameState.deadline = message.game.phase_deadline || null;
        updateUI(uiCallbacks);
        updatePanicModeUI();
        updateSoftPanicModeUI();
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

      // Play TTS for the revealed submission
      if (message.submission && tts) {
        tts.speak(message.submission.display_text, {
          rate: 0.9,
          pitch: 1.0,
          onError: (err) => console.warn("[TTS] Speech error:", err),
        });
      }
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

    case "soft_panic_mode_update":
      gameState.softPanicMode = message.enabled;
      updateSoftPanicModeUI();
      showAlert(
        message.enabled ? "Prompt-Panik aktiviert" : "Prompt-Panik deaktiviert",
        message.enabled ? "warning" : "success",
      );
      break;

    case "venue_only_mode_update":
      gameState.venueOnlyMode = message.enabled;
      updateVenueOnlyModeUI();
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

    case "host_connection_stats":
      document.getElementById("connectedPlayers").textContent = message.players;
      document.getElementById("connectedAudience").textContent =
        message.audience;
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

    // Trivia messages
    case "host_trivia_questions":
      console.log("Received host_trivia_questions:", message);
      gameState.triviaQuestions = message.questions || [];
      // Look up the active question by ID from the questions list
      if (message.active_trivia_id) {
        gameState.activeTrivia =
          gameState.triviaQuestions.find(
            (q) => q.id === message.active_trivia_id,
          ) || null;
      } else {
        gameState.activeTrivia = null;
      }
      gameState.activeTriviaVoteCount = message.active_trivia_votes || 0;
      console.log(
        "Updated activeTriviaVoteCount:",
        gameState.activeTriviaVoteCount,
      );
      updateTriviaUI();
      break;

    case "trivia_result":
      // Trivia was resolved - results are now showing on beamer/audience
      gameState.triviaResultShowing = true;
      gameState.activeTrivia = null; // Active trivia is cleared after resolve
      updateTriviaUI();
      break;

    case "trivia_clear":
      // Trivia was cleared (either by host or phase change)
      gameState.triviaResultShowing = false;
      gameState.activeTrivia = null;
      updateTriviaUI();
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

/**
 * Setup event delegation for data-action attributes
 * This replaces inline onclick handlers to prevent XSS vulnerabilities
 */
function setupEventDelegation() {
  document.addEventListener("click", (e) => {
    const btn = e.target.closest("[data-action]");
    if (!btn) return;

    const action = btn.dataset.action;

    // Stop propagation for action buttons inside interactive elements
    e.stopPropagation();

    switch (action) {
      // Player actions
      case "copy-token":
        copyToClipboard(btn.dataset.token);
        break;
      case "remove-player":
        removePlayer(btn.dataset.playerId, btn.dataset.playerName, wsConn);
        break;

      // Prompt actions
      case "queue-prompt":
        queuePrompt(btn.dataset.id, wsConn);
        break;
      case "unqueue-prompt":
        unqueuePrompt(btn.dataset.id, wsConn);
        break;
      case "delete-prompt":
        deletePrompt(btn.dataset.id, wsConn);
        break;
      case "shadowban-audience":
        shadowbanAudience(btn.dataset.voterId, wsConn);
        break;
      case "shadowban-submitters":
        shadowbanPromptSubmitters(btn.dataset.id, wsConn);
        break;

      // Submission actions
      case "select-ai":
        selectAiSubmission(btn.dataset.submissionId, wsConn);
        break;
      case "remove-submission":
        removeSubmission(btn.dataset.submissionId, wsConn);
        break;
      case "mark-duplicate":
        markDuplicate(btn.dataset.submissionId, wsConn);
        break;
      case "edit-submission":
        editSubmission(btn.dataset.submissionId, wsConn);
        break;

      // Manual winner (panic mode)
      case "set-manual-winner":
        setManualWinner(btn.dataset.winnerType, btn.dataset.submissionId);
        break;

      // Voting controls
      case "reveal-vote-labels":
        revealVoteLabels();
        break;
    }
  });
}

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

function revealVoteLabels() {
  wsConn.send({ t: "host_reveal_vote_labels" });
  log("Antworten auf Beamer aufgedeckt", "info");
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

function clearPromptPool() {
  if (
    confirm(
      "Willst du alle Prompts aus dem Pool löschen? Das kann nicht rückgängig gemacht werden.",
    )
  ) {
    wsConn.send({ t: "host_clear_prompt_pool" });
  }
}

function clearAudienceMembers() {
  if (
    confirm(
      "Willst du alle Publikums-Daten löschen (Namen, IDs)? Das gibt Speicher frei, setzt aber das Leaderboard zurück.",
    )
  ) {
    wsConn.send({ t: "host_clear_audience_members" });
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
        html += `<button class="secondary" data-action="set-manual-winner" data-winner-type="ai" data-submission-id="${escapeHtml(sub.id)}" title="${escapeHtml(sub.display_text)}">${idx + 1}. ${escapeHtml(shortText)}</button>`;
      });
      html += "</div>";

      html +=
        '<div style="margin-bottom: 15px;"><strong>Als Lustigster markieren:</strong></div>';
      html += '<div class="button-group">';
      gameState.submissions.forEach((sub, idx) => {
        const shortText =
          sub.display_text.substring(0, 30) +
          (sub.display_text.length > 30 ? "..." : "");
        html += `<button class="secondary" data-action="set-manual-winner" data-winner-type="funny" data-submission-id="${escapeHtml(sub.id)}" title="${escapeHtml(sub.display_text)}">${idx + 1}. ${escapeHtml(shortText)}</button>`;
      });
      html += "</div>";

      manualWinnerButtons.innerHTML = html;
    }
  }
}

function toggleSoftPanicMode() {
  const newState = !gameState.softPanicMode;
  if (
    newState &&
    !confirm(
      "Prompt-Panik aktivieren?\n\nDas Publikum kann dann keine Prompt-Vorschläge mehr einreichen. Normale Abstimmungen funktionieren weiterhin.",
    )
  ) {
    return;
  }
  wsConn.send({ t: "host_toggle_soft_panic_mode", enabled: newState });
}

function updateSoftPanicModeUI() {
  const softPanicBtn = document.getElementById("softPanicModeBtn");
  const softPanicStatus = document.getElementById("softPanicStatus");

  if (softPanicBtn) {
    softPanicBtn.textContent = gameState.softPanicMode
      ? "Prompt-Panik DEAKTIVIEREN"
      : "Prompt-Panik aktivieren";
    softPanicBtn.classList.toggle("active", gameState.softPanicMode);
  }

  if (softPanicStatus) {
    softPanicStatus.textContent = gameState.softPanicMode ? "AKTIV" : "Inaktiv";
    softPanicStatus.classList.toggle("active", gameState.softPanicMode);
  }
}

function toggleVenueOnlyMode() {
  const newState = !gameState.venueOnlyMode;
  if (
    newState &&
    !confirm(
      "Venue-Only Modus aktivieren?\n\nNur Personen mit IPs aus den konfigurierten Bereichen (VENUE_IP_RANGES) können dann beitreten.",
    )
  ) {
    return;
  }
  wsConn.send({ t: "host_toggle_venue_only_mode", enabled: newState });
}

function updateVenueOnlyModeUI() {
  const venueBtn = document.getElementById("venueOnlyModeBtn");
  const venueStatus = document.getElementById("venueOnlyStatus");

  if (venueBtn) {
    venueBtn.textContent = gameState.venueOnlyMode
      ? "Venue-Only DEAKTIVIEREN"
      : "Venue-Only aktivieren";
    venueBtn.classList.toggle("venue", !gameState.venueOnlyMode);
    venueBtn.classList.toggle("venue-active", gameState.venueOnlyMode);
  }

  if (venueStatus) {
    venueStatus.textContent = gameState.venueOnlyMode ? "AKTIV" : "Inaktiv";
    venueStatus.style.color = gameState.venueOnlyMode ? "#22c55e" : "inherit";
  }
}

// Trivia Functions - Dynamic choice count (2-4)
const TRIVIA_CHOICE_LABELS = ["A", "B", "C", "D"];
let triviaChoiceCount = 2; // Start with 2 choices
const triviaChoiceModes = ["text", "text", "text", "text"]; // Track mode per choice

function toggleTriviaChoiceMode(index) {
  triviaChoiceModes[index] =
    triviaChoiceModes[index] === "text" ? "image" : "text";
  renderTriviaChoices();
}

function renderTriviaChoices() {
  const container = document.getElementById("triviaChoicesContainer");
  if (!container) return;

  let html = "";
  for (let i = 0; i < triviaChoiceCount; i++) {
    const mode = triviaChoiceModes[i] || "text";
    const existingText =
      document.getElementById(`triviaChoiceText${i}`)?.value || "";
    const existingImage =
      document.getElementById(`triviaChoiceImage${i}`)?.value || "";

    html += `
      <div style="display: flex; flex-direction: column; gap: 6px; margin-bottom: 12px; padding: 10px; background: rgba(255,255,255,0.05); border-radius: 6px;" data-choice-index="${i}">
        <div style="display: flex; gap: 10px; align-items: center;">
          <input type="radio" name="triviaCorrect" value="${i}" id="triviaCorrect${i}" ${i === 0 ? "checked" : ""} style="width: 18px; height: 18px;">
          <label for="triviaCorrect${i}" style="font-weight: bold; min-width: 20px;">${TRIVIA_CHOICE_LABELS[i]}:</label>
          <button type="button" onclick="toggleTriviaChoiceMode(${i})" style="padding: 4px 8px; font-size: 0.85em; background: ${mode === "text" ? "#228be6" : "#40c057"};">
            ${mode === "text" ? "Text" : "Bild"}
          </button>
          ${triviaChoiceCount > 2 ? `<button type="button" onclick="removeTriviaChoice(${i})" style="padding: 4px 8px; background: #e03131; margin-left: auto;">X</button>` : ""}
        </div>
        ${mode === "text" ? `<input type="text" id="triviaChoiceText${i}" placeholder="Antwort ${TRIVIA_CHOICE_LABELS[i]}" value="${escapeHtml(existingText)}" style="margin: 0;">` : `<input type="text" id="triviaChoiceImage${i}" placeholder="https://example.com/image.jpg" value="${escapeHtml(existingImage)}" style="margin: 0;">`}
      </div>
    `;
  }
  container.innerHTML = html;

  // Show/hide add button
  const addBtn = document.getElementById("addTriviaChoiceBtn");
  if (addBtn) {
    addBtn.style.display = triviaChoiceCount >= 4 ? "none" : "inline-block";
  }
}

function addTriviaChoice() {
  if (triviaChoiceCount >= 4) return;
  triviaChoiceCount++;
  renderTriviaChoices();
}

function removeTriviaChoice(index) {
  if (triviaChoiceCount <= 2) return;

  // Save current values and modes before re-rendering
  const savedChoices = [];
  for (let i = 0; i < triviaChoiceCount; i++) {
    if (i !== index) {
      savedChoices.push({
        mode: triviaChoiceModes[i],
        text: document.getElementById(`triviaChoiceText${i}`)?.value || "",
        image: document.getElementById(`triviaChoiceImage${i}`)?.value || "",
      });
    }
  }

  triviaChoiceCount--;

  // Restore modes
  for (let i = 0; i < savedChoices.length; i++) {
    triviaChoiceModes[i] = savedChoices[i].mode;
  }

  renderTriviaChoices();

  // Restore values
  for (let i = 0; i < savedChoices.length; i++) {
    const textInput = document.getElementById(`triviaChoiceText${i}`);
    const imageInput = document.getElementById(`triviaChoiceImage${i}`);
    if (textInput) textInput.value = savedChoices[i].text;
    if (imageInput) imageInput.value = savedChoices[i].image;
  }
}

function addTriviaQuestion() {
  const questionText = document
    .getElementById("triviaQuestionText")
    ?.value?.trim();
  if (!questionText) {
    showAlert("Bitte gib eine Frage ein", "error");
    return;
  }

  const questionImageUrl =
    document.getElementById("triviaQuestionImageUrl")?.value?.trim() || null;

  const choices = [];
  for (let i = 0; i < triviaChoiceCount; i++) {
    const mode = triviaChoiceModes[i] || "text";
    const isCorrect =
      document.querySelector(`input[name="triviaCorrect"]:checked`)?.value ===
      String(i);

    if (mode === "text") {
      const choiceText = document
        .getElementById(`triviaChoiceText${i}`)
        ?.value?.trim();
      if (!choiceText) {
        showAlert(
          `Bitte fülle Antwort ${TRIVIA_CHOICE_LABELS[i]} aus`,
          "error",
        );
        return;
      }
      choices.push({
        text: choiceText,
        image_url: null,
        is_correct: isCorrect,
      });
    } else {
      const imageUrl = document
        .getElementById(`triviaChoiceImage${i}`)
        ?.value?.trim();
      if (!imageUrl) {
        showAlert(
          `Bitte gib eine Bild-URL für Antwort ${TRIVIA_CHOICE_LABELS[i]} ein`,
          "error",
        );
        return;
      }
      choices.push({ text: "", image_url: imageUrl, is_correct: isCorrect });
    }
  }

  wsConn.send({
    t: "host_add_trivia_question",
    question: questionText,
    image_url: questionImageUrl,
    choices: choices,
  });

  // Clear form and reset to 2 choices
  document.getElementById("triviaQuestionText").value = "";
  document.getElementById("triviaQuestionImageUrl").value = "";
  triviaChoiceCount = 2;
  triviaChoiceModes[0] = "text";
  triviaChoiceModes[1] = "text";
  triviaChoiceModes[2] = "text";
  triviaChoiceModes[3] = "text";
  renderTriviaChoices();
  document.getElementById("triviaCorrect0").checked = true;

  showAlert("Trivia-Frage hinzugefügt", "success");
}

function presentTrivia(questionId) {
  if (gameState.phase !== "WRITING") {
    showAlert(
      "Trivia kann nur während der WRITING-Phase präsentiert werden",
      "error",
    );
    return;
  }
  if (gameState.activeTrivia) {
    showAlert(
      "Es ist bereits eine Trivia-Frage aktiv. Löse sie zuerst auf oder blende sie aus.",
      "error",
    );
    return;
  }
  wsConn.send({
    t: "host_present_trivia",
    question_id: questionId,
  });
  showAlert("Trivia-Frage wird präsentiert", "success");
}

function resolveTrivia() {
  if (!gameState.activeTrivia) {
    showAlert("Keine aktive Trivia-Frage", "error");
    return;
  }
  wsConn.send({ t: "host_resolve_trivia" });
  showAlert("Trivia aufgelöst - Ergebnis wird angezeigt", "success");
}

function clearTrivia() {
  if (!gameState.activeTrivia) {
    showAlert("Keine aktive Trivia-Frage", "error");
    return;
  }
  wsConn.send({ t: "host_clear_trivia" });
  showAlert("Trivia ausgeblendet", "success");
}

function removeTriviaQuestion(questionId) {
  if (!confirm("Willst du diese Trivia-Frage wirklich löschen?")) {
    return;
  }
  wsConn.send({
    t: "host_remove_trivia_question",
    question_id: questionId,
  });
}

function updateTriviaUI() {
  const activeTriviaCard = document.getElementById("activeTriviaCard");
  const triviaResultCard = document.getElementById("triviaResultCard");
  const triviaQuestionCount = document.getElementById("triviaQuestionCount");
  const triviaQuestionsList = document.getElementById("triviaQuestionsList");
  const activeTriviaText = document.getElementById("activeTriviaText");
  const activeTriviaChoices = document.getElementById("activeTriviaChoices");
  const activeTriviaVoteCount = document.getElementById(
    "activeTriviaVoteCount",
  );

  // Update question count
  if (triviaQuestionCount) {
    triviaQuestionCount.textContent = gameState.triviaQuestions.length;
  }

  // Update active trivia card (when question is being presented, before resolve)
  if (activeTriviaCard) {
    if (gameState.activeTrivia) {
      activeTriviaCard.style.display = "block";
      if (activeTriviaText) {
        activeTriviaText.textContent = gameState.activeTrivia.question;
      }
      if (activeTriviaChoices) {
        let html = "";
        gameState.activeTrivia.choices.forEach((choice, idx) => {
          const correctClass = choice.is_correct
            ? 'style="color: #51cf66; font-weight: bold;"'
            : "";
          const displayText = choice.image_url
            ? "[Bild]"
            : escapeHtml(choice.text);
          html += `<div ${correctClass}>${TRIVIA_CHOICE_LABELS[idx]}: ${displayText}${choice.is_correct ? " ✓" : ""}</div>`;
        });
        activeTriviaChoices.innerHTML = html;
      }
      if (activeTriviaVoteCount) {
        activeTriviaVoteCount.textContent = gameState.activeTriviaVoteCount;
      }
    } else {
      activeTriviaCard.style.display = "none";
    }
  }

  // Update trivia result card (when results are showing, after resolve)
  if (triviaResultCard) {
    triviaResultCard.style.display = gameState.triviaResultShowing
      ? "block"
      : "none";
  }

  // Update questions list
  if (triviaQuestionsList) {
    if (gameState.triviaQuestions.length === 0) {
      triviaQuestionsList.innerHTML =
        '<p style="opacity: 0.6;">Noch keine Trivia-Fragen vorhanden. Füge oben eine hinzu.</p>';
    } else {
      let html = "";
      gameState.triviaQuestions.forEach((q) => {
        const isActive = gameState.activeTrivia?.id === q.id;
        const canPresent =
          gameState.phase === "WRITING" &&
          !gameState.activeTrivia &&
          !gameState.triviaResultShowing;
        const questionImageHtml = q.image_url
          ? `<img src="${escapeHtml(q.image_url)}" style="max-width: 100px; max-height: 60px; border-radius: 4px; margin-bottom: 8px;" alt="Fragen-Bild">`
          : "";
        const choicesHtml = q.choices
          .map((c, i) => {
            const displayText = c.image_url ? "[Bild]" : escapeHtml(c.text);
            return `<div ${c.is_correct ? 'style="color: #51cf66;"' : ""}>${TRIVIA_CHOICE_LABELS[i]}: ${displayText}${c.is_correct ? " ✓" : ""}</div>`;
          })
          .join("");
        html += `
          <div class="trivia-question-card" style="background: rgba(255,255,255,0.05); border-radius: 8px; padding: 15px; margin-bottom: 10px; ${isActive ? "border: 2px solid #ffd43b;" : ""}">
            <div style="font-weight: bold; margin-bottom: 10px;">${escapeHtml(q.question)}</div>
            ${questionImageHtml}
            <div style="margin-bottom: 10px; font-size: 0.9em;">
              ${choicesHtml}
            </div>
            <div class="button-group">
              <button onclick="presentTrivia('${escapeHtml(q.id)}')" class="primary" ${canPresent ? "" : "disabled"} title="${canPresent ? "Frage praesentieren" : "Nur waehrend WRITING und ohne aktive Trivia"}">Praesentieren</button>
              <button onclick="removeTriviaQuestion('${escapeHtml(q.id)}')" class="danger" ${isActive ? "disabled" : ""}>Loeschen</button>
            </div>
          </div>
        `;
      });
      triviaQuestionsList.innerHTML = html;
    }
  }
}

/**
 * Clear trivia result from beamer/audience
 */
function clearTriviaResult() {
  wsConn.send({
    t: "host_clear_trivia",
  });
}

/**
 * Check if audio/TTS is available without user interaction
 */
function checkAudioAccess() {
  if (!window.speechSynthesis) {
    showAudioUnlockButton();
    return;
  }

  const testUtterance = new SpeechSynthesisUtterance("");
  testUtterance.volume = 0;

  testUtterance.onerror = (event) => {
    if (event.error === "not-allowed") {
      showAudioUnlockButton();
    }
  };

  window.speechSynthesis.speak(testUtterance);
}

function showAudioUnlockButton() {
  const container = document.getElementById("audioUnlockContainer");
  if (container) container.style.display = "block";
}

function hideAudioUnlockButton() {
  const container = document.getElementById("audioUnlockContainer");
  if (container) container.style.display = "none";
}

function setupAudioUnlockButton() {
  const btn = document.getElementById("unlockAudioBtn");
  if (!btn) return;

  btn.addEventListener("click", () => {
    const utterance = new SpeechSynthesisUtterance("");
    window.speechSynthesis.speak(utterance);
    hideAudioUnlockButton();
  });
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
    setSelectedModel,
    getSelectedModel,

    // Submissions
    markDuplicate: (id) => markDuplicate(id, wsConn),
    editSubmission: (id) => editSubmission(id, wsConn),
    setRevealOrder: () => setRevealOrder(wsConn),

    // Game control
    resetGame,
    clearPromptPool,
    clearAudienceMembers,
    togglePanicMode,
    toggleSoftPanicMode,
    setManualWinner,
    extendTimer,

    // Venue mode
    toggleVenueOnlyMode,

    // Trivia
    addTriviaQuestion,
    addTriviaChoice,
    removeTriviaChoice,
    toggleTriviaChoiceMode,
    presentTrivia,
    resolveTrivia,
    clearTrivia,
    clearTriviaResult,
    removeTriviaQuestion,

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
    copyPlayerUrl,
    copyToClipboard,
  });
}
