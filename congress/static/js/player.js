/**
 * Player-specific JavaScript
 */

let wsConn = null;
let playerToken = null;
let playerId = null;
let playerName = null;
let currentPhase = null;
let currentPrompt = null;
let hasSubmitted = false;
let playerTimer = null;
let pendingTypoCheck = null; // Track pending typo check data
const MAX_CHARS = 500;
const WARN_THRESHOLD = 450;
const STORAGE_KEY = "gptdash_player_token";

// Initialize
function init() {
  // Check if player token is in URL or localStorage
  const params = new URLSearchParams(window.location.search);
  const urlToken = params.get("token");
  const storedToken = localStorage.getItem(STORAGE_KEY);

  if (urlToken) {
    document.getElementById("tokenInput").value = urlToken;
    playerToken = urlToken;
  } else if (storedToken) {
    document.getElementById("tokenInput").value = storedToken;
    playerToken = storedToken;
  }

  // Setup char counter
  const answerInput = document.getElementById("answerInput");
  if (answerInput) {
    answerInput.addEventListener("input", updateCharCounter);
  }

  // Initialize timer
  playerTimer = new CountdownTimer("playerTimer");

  // Connect to WebSocket with token for state recovery
  wsConn = new WSConnection(
    "player",
    handleMessage,
    updateConnectionStatus,
    playerToken,
  );
  wsConn.connect();
}

function requireConnection(errorElementId) {
  if (!wsConn || !wsConn.isConnected()) {
    if (errorElementId) {
      showError(errorElementId, "Verbinde noch mit dem Spiel-Server...");
    }
    return false;
  }
  return true;
}

function handleMessage(message) {
  switch (message.t) {
    case "welcome":
      if (message.game) {
        currentPhase = message.game.phase;
        // Start timer if in WRITING phase with deadline
        if (
          currentPhase === "WRITING" &&
          message.game.phase_deadline &&
          message.server_now
        ) {
          playerTimer.start(message.game.phase_deadline, message.server_now);
        }
      }
      break;

    case "player_state":
      // State recovery on reconnect
      playerId = message.player_id;
      playerName = message.display_name;
      hasSubmitted = message.has_submitted;
      // Recover prompt from state if available
      if (message.current_prompt) {
        currentPrompt = message.current_prompt;
      }

      if (playerName) {
        // Player is registered, go to appropriate screen
        updateConnectionStatus(true, `Beigetreten als ${playerName}`);
        if (hasSubmitted) {
          showScreen("submittedScreen");
        } else if (currentPhase === "WRITING") {
          showWritingScreen();
        } else {
          showScreen("waitingScreen");
        }
      } else if (playerToken) {
        // Token valid but not registered yet
        showScreen("registerScreen");
      }
      break;

    case "phase":
      currentPhase = message.phase;
      // Update timer for WRITING phase
      if (
        currentPhase === "WRITING" &&
        message.deadline &&
        message.server_now
      ) {
        playerTimer.start(message.deadline, message.server_now);
      } else {
        playerTimer.stop();
        playerTimer.hide();
        const timerEl = document.getElementById("playerTimer");
        if (timerEl) timerEl.textContent = "--:--";
      }
      // Update prompt if included in phase message (sent during WRITING transition)
      if (message.prompt) {
        currentPrompt = message.prompt;
      }
      updateScreen(message.phase);
      break;

    case "deadline_update":
      // Update timer when deadline is extended
      if (playerTimer && message.deadline && message.server_now) {
        playerTimer.updateDeadline(message.deadline, message.server_now);
      }
      break;

    case "prompt_selected":
      currentPrompt = message.prompt;
      break;

    case "round_started":
      // Reset submission state for new round
      hasSubmitted = false;
      if (message.round?.selected_prompt) {
        currentPrompt = message.round.selected_prompt;
      }
      break;

    case "submission_confirmed":
      hasSubmitted = true;
      // If we were showing the typo correction screen, stay there
      // Otherwise show submitted screen
      if (
        document.getElementById("typoCheckScreen")?.classList.contains("active")
      ) {
        // We just accepted a correction, go to submitted screen
        showScreen("submittedScreen");
        pendingTypoCheck = null;
      } else if (!pendingTypoCheck) {
        // Normal submission confirmed, now request typo check
        showScreen("submittedScreen");
        // Request typo check in background
        const answerInput = document.getElementById("answerInput");
        if (answerInput?.value.trim()) {
          requestTypoCheck(answerInput.value.trim());
        }
      } else {
        // Submission confirmed while typo check is pending - show submitted
        showScreen("submittedScreen");
      }
      break;

    case "typo_check_result":
      handleTypoCheckResult(message);
      break;

    case "player_registered":
      // Capture player_id from registration
      playerId = message.player_id;
      break;

    case "submission_rejected":
      // Only handle if it's for this player
      if (message.player_id === playerId) {
        hasSubmitted = false;

        // Clear the answer input
        const answerInput = document.getElementById("answerInput");
        if (answerInput) {
          answerInput.value = "";
          updateCharCounter();
        }

        // Show writing screen with error
        showWritingScreen();
        showError(
          "submitError",
          message.reason === "duplicate"
            ? "Diese Antwort existiert schon. Bitte gib eine andere Antwort ein."
            : "Deine Antwort wurde abgelehnt. Bitte versuch es erneut.",
        );
      }
      break;

    case "error":
      // Check for duplicate error from automatic detection
      if (message.msg === "DUPLICATE_EXACT") {
        showError(
          "submitError",
          "Diese Antwort existiert schon. Bitte gib eine andere Antwort ein.",
        );
      } else {
        handleError(message.msg);
      }
      break;
  }
}

function joinGame() {
  const tokenInput = document.getElementById("tokenInput");
  const token = tokenInput.value.trim();

  if (!token) {
    showError("joinError", "Bitte gib einen Spieler-Token ein");
    return;
  }

  playerToken = token;
  // Store token for reconnection
  localStorage.setItem(STORAGE_KEY, token);
  // Update connection with token for future reconnects
  wsConn.setToken(token);

  if (!requireConnection("joinError")) {
    return;
  }

  // Send join message
  const sent = wsConn.send({
    t: "join",
    room_token: token,
  });

  if (!sent) {
    showError(
      "joinError",
      "Server nicht erreichbar. Versuch's gleich nochmal.",
    );
    return;
  }

  hideError("joinError");

  // Move to register screen
  showScreen("registerScreen");
}

function registerName() {
  const nameInput = document.getElementById("nameInput");
  const name = nameInput.value.trim();

  if (!name) {
    showError("registerError", "Bitte gib einen Namen ein");
    return;
  }

  if (name.length < 2) {
    showError("registerError", "Name muss mindestens 2 Zeichen haben");
    return;
  }

  playerName = name;

  if (!requireConnection("registerError")) {
    return;
  }

  // Send register message
  const sent = wsConn.send({
    t: "register_player",
    player_token: playerToken,
    display_name: name,
  });

  if (!sent) {
    showError("registerError", "Verbindung verloren. Versuch's nochmal.");
    return;
  }

  // Update status
  updateConnectionStatus(true, `Beigetreten als ${name}`);
  hideError("registerError");

  // Show appropriate screen based on current game phase
  if (currentPhase === "WRITING") {
    showWritingScreen();
  } else {
    showScreen("waitingScreen");
  }
}

function submitAnswer() {
  const answerInput = document.getElementById("answerInput");
  const answer = answerInput.value.trim();

  if (!answer) {
    showError("submitError", "Bitte gib eine Antwort ein");
    return;
  }

  if (answer.length < 10) {
    showError("submitError", "Antwort muss mindestens 10 Zeichen haben");
    return;
  }

  if (!requireConnection("submitError")) {
    return;
  }

  // Send submission
  const sent = wsConn.send({
    t: "submit_answer",
    player_token: playerToken,
    text: answer,
  });

  if (!sent) {
    showError("submitError", "Verbindung verloren. Versuch's nochmal.");
    return;
  }

  // Don't show submitted screen immediately - wait for server response
  // The server will send either submission_confirmed or error
  hideError("submitError");
}

function editAnswer() {
  showScreen("writingScreen");
}

function updateScreen(phase) {
  console.log("Phase update:", phase);

  switch (phase) {
    case "LOBBY":
    case "PROMPT_SELECTION":
      if (playerName) {
        showScreen("waitingScreen");
      }
      break;

    case "WRITING":
      showWritingScreen();
      break;

    case "REVEAL":
    case "VOTING":
    case "RESULTS":
    case "PODIUM":
      showScreen("lockedScreen");
      break;

    case "INTERMISSION":
      showScreen("waitingScreen");
      break;
  }
}

function showWritingScreen() {
  if (currentPrompt) {
    const promptEl = document.getElementById("promptText");
    const promptImageEl = document.getElementById("promptImage");

    // Handle text
    if (promptEl) {
      const text =
        currentPrompt.text ||
        (typeof currentPrompt === "string" ? currentPrompt : "");
      promptEl.textContent = text || "(Bildfrage - siehe Bild oben)";
      promptEl.style.display = text ? "block" : "none";
    }

    // Handle image
    if (promptImageEl) {
      if (currentPrompt.image_url) {
        promptImageEl.innerHTML = `<img src="${escapeHtml(currentPrompt.image_url)}" alt="Prompt-Bild" class="prompt-image-display">`;
        promptImageEl.style.display = "block";
      } else {
        promptImageEl.innerHTML = "";
        promptImageEl.style.display = "none";
      }
    }
  }
  showScreen("writingScreen");
}

function updateCharCounter() {
  const answerInput = document.getElementById("answerInput");
  const counter = document.getElementById("charCounter");
  const length = answerInput.value.length;

  counter.textContent = `${length} / ${MAX_CHARS}`;

  counter.classList.remove("warning", "error");

  if (length >= MAX_CHARS) {
    counter.classList.add("error");
  } else if (length >= WARN_THRESHOLD) {
    counter.classList.add("warning");
  }
}

function handleError(message) {
  console.error("Server error:", message);
  // Show error in current screen context
  const currentScreen = document.querySelector(".screen.active");
  if (currentScreen) {
    const errorEl = currentScreen.querySelector(".error-message");
    if (errorEl) {
      showError(errorEl.id, message);
    }
  }
}

function requestTypoCheck(text) {
  if (!wsConn || !wsConn.isConnected()) {
    console.warn("Cannot request typo check: not connected");
    return;
  }

  pendingTypoCheck = { original: text };

  wsConn.send({
    t: "request_typo_check",
    player_token: playerToken,
    text: text,
  });

  console.log("Requested typo check for submission");
}

function handleTypoCheckResult(message) {
  console.log("Typo check result:", message.has_changes);

  if (!message.has_changes) {
    // No changes needed, stay on submitted screen
    pendingTypoCheck = null;
    return;
  }

  // Store the correction data
  pendingTypoCheck = {
    original: message.original,
    corrected: message.corrected,
  };

  // Show the comparison UI
  showTypoCheckScreen(message.original, message.corrected);
}

function showTypoCheckScreen(original, corrected) {
  // Update the comparison texts
  const originalEl = document.getElementById("originalText");
  const correctedEl = document.getElementById("correctedText");

  if (originalEl) {
    originalEl.textContent = original;
  }
  if (correctedEl) {
    correctedEl.textContent = corrected;
  }

  showScreen("typoCheckScreen");
}

function acceptCorrection() {
  if (!pendingTypoCheck || !pendingTypoCheck.corrected) {
    console.error("No pending correction to accept");
    return;
  }

  if (!wsConn || !wsConn.isConnected()) {
    showError("typoError", "Verbindung verloren. Bitte versuche es erneut.");
    return;
  }

  // We need to get the submission ID from the server
  // Since we already submitted, we send an update
  // The server will find our submission by player token
  wsConn.send({
    t: "submit_answer",
    player_token: playerToken,
    text: pendingTypoCheck.corrected,
  });

  // Show submitting state
  showScreen("submittedScreen");
  pendingTypoCheck = null;
}

function rejectCorrection() {
  // Keep original (already submitted), just close the comparison
  pendingTypoCheck = null;
  showScreen("submittedScreen");
}

function editManually() {
  // Go back to writing screen with the original text
  const answerInput = document.getElementById("answerInput");
  if (answerInput && pendingTypoCheck) {
    answerInput.value = pendingTypoCheck.original;
    updateCharCounter();
  }
  pendingTypoCheck = null;
  hasSubmitted = false; // Allow resubmission
  showWritingScreen();
}

if (typeof window !== "undefined") {
  Object.assign(window, {
    joinGame,
    registerName,
    submitAnswer,
    editAnswer,
    acceptCorrection,
    rejectCorrection,
    editManually,
  });
}

// Initialize on page load
init();
