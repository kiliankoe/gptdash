/**
 * Host panel UI utilities
 */

import { gameState } from "./state.js";
import { escapeHtml, copyToClipboard, QRCodeManager } from "../common.js";

/**
 * Update connection status display
 */
export function updateStatus(connected) {
  const dot = document.getElementById("statusDot");
  const text = document.getElementById("statusText");

  if (connected) {
    dot.classList.add("connected");
    text.textContent = "Verbunden";
  } else {
    dot.classList.remove("connected");
    text.textContent = "Nicht verbunden";
  }
}

/**
 * Update main UI displays
 */
export function updateUI(callbacks) {
  // Update header displays
  document.getElementById("phaseDisplay").textContent =
    `Phase: ${gameState.phase}`;
  document.getElementById("roundDisplay").textContent =
    `Round: ${gameState.roundNo}`;

  // Update overview
  document.getElementById("overviewPhase").textContent = gameState.phase;
  document.getElementById("overviewRound").textContent = gameState.roundNo;
  document.getElementById("overviewPlayers").textContent =
    callbacks?.getPlayerCount?.() ?? 0;
  document.getElementById("overviewSubmissions").textContent =
    gameState.submissions.length;

  // Update phase transition buttons
  updatePhaseButtons();

  // Update current round info
  updateCurrentRoundInfo();

  // Update overview helpers
  callbacks?.updateOverviewFlow?.();
  callbacks?.updateOverviewRevealStatus?.();
}

/**
 * Update phase transition buttons based on valid transitions from server
 */
export function updatePhaseButtons() {
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

/**
 * Update current round info display
 */
export function updateCurrentRoundInfo() {
  const containers = [
    document.getElementById("overviewCurrentRoundInfo"),
  ].filter(Boolean);
  if (containers.length === 0) return;

  if (!gameState.currentPrompt) {
    containers.forEach((container) => {
      container.innerHTML =
        '<p style="opacity: 0.6;">Keine aktive Runde. Fge einen Prompt hinzu und starte eine Runde.</p>';
    });
    return;
  }

  const prompt = gameState.currentPrompt;
  let html = `<div style="background: rgba(255,255,255,0.05); padding: 10px; border-radius: 8px;">`;
  html += `<div style="font-size: 0.9em; opacity: 0.7; margin-bottom: 5px;">Runde ${gameState.roundNo} - Aktueller Prompt:</div>`;

  if (prompt.image_url) {
    html += `<img src="${escapeHtml(prompt.image_url)}" alt="Prompt-Bild" style="max-width: 100%; max-height: 150px; border-radius: 4px; margin-bottom: 8px;">`;
  }

  if (prompt.text) {
    html += `<div style="font-size: 1.1em; font-weight: 500;">${escapeHtml(prompt.text)}</div>`;
  } else if (prompt.image_url) {
    html += `<div style="font-style: italic; opacity: 0.7;">(Nur Bild)</div>`;
  }

  html += `</div>`;
  containers.forEach((container) => {
    container.innerHTML = html;
  });
}

/**
 * Show alert message
 */
export function showAlert(message, type = "info") {
  const container = document.getElementById("overviewAlert");
  if (!container) return;
  container.innerHTML = "";

  const alert = document.createElement("div");
  alert.className = `alert ${type}`;
  alert.textContent = message;
  container.appendChild(alert);

  // Auto-hide after 5 seconds
  setTimeout(() => {
    container.innerHTML = "";
  }, 5000);
}

/**
 * Generate QR codes for joining the game
 */
export function generateJoinQRCodes() {
  // Generate QR code for player join
  const playerUrl = QRCodeManager.getPlayerJoinUrl();
  QRCodeManager.generate("playerQRCode", playerUrl, {
    width: 200,
    height: 200,
  });

  // Update URL display
  const urlEl = document.getElementById("playerJoinUrl");
  if (urlEl) {
    urlEl.textContent = playerUrl;
  }
}

/**
 * Copy player join URL to clipboard
 */
export function copyPlayerUrl() {
  const url = QRCodeManager.getPlayerJoinUrl();
  copyToClipboard(url);
  showAlert("URL in Zwischenablage kopiert!", "success");
}

/**
 * Update scores display
 */
export function updateScores() {
  // Player scores
  const playerContainer = document.getElementById("playerScores");
  playerContainer.innerHTML = "";

  if (gameState.scores.players.length === 0) {
    playerContainer.innerHTML =
      '<p style="opacity: 0.6;">Noch keine Punkte</p>';
  } else {
    gameState.scores.players.forEach((score, idx) => {
      const displayName = score.display_name || score.ref_id.substring(0, 12);
      playerContainer.innerHTML += `
        <div class="info-item">
          <div class="label">${idx + 1}. ${escapeHtml(displayName)}</div>
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
      const displayName = score.display_name || score.ref_id.substring(0, 12);
      audienceContainer.innerHTML += `
        <div class="info-item">
          <div class="label">${idx + 1}. ${escapeHtml(displayName)}</div>
          <div class="value">${score.total} Pkt</div>
        </div>
      `;
    });
  }
}
