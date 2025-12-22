/**
 * Host panel player management
 */

import { gameState } from "./state.js";
import { escapeHtml } from "../common.js";
import { showAlert } from "./ui.js";

/**
 * Get current player count
 */
export function getPlayerCount() {
  if (gameState.playerStatus.length > 0) return gameState.playerStatus.length;
  return gameState.players.length;
}

/**
 * Update players list in both containers
 */
export function updatePlayersList() {
  updatePlayersListInto("playerTokensList");
  updatePlayersListInto("overviewPlayerTokensList");
}

/**
 * Update players list into a specific container
 */
function updatePlayersListInto(containerId) {
  const container = document.getElementById(containerId);
  if (!container) return;
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
        statusBadge = "Eingereicht";
        statusClass = "submitted";
        break;
      case "checking_typos":
        statusBadge = "Prft...";
        statusClass = "checking";
        break;
      default:
        statusBadge = "Wartet";
        statusClass = "waiting";
    }

    const playerId = player.id || `unknown_${token}`;
    div.innerHTML = `
      <div class="player-info">
        <div class="player-header">
          <span class="player-name">${escapeHtml(name)}</span>
          <span class="status-badge ${statusClass}">${statusBadge}</span>
        </div>
        <div class="player-token">
          <span class="token">${escapeHtml(token)}</span>
          <button class="copy-btn" data-action="copy-token" data-token="${escapeHtml(token)}" title="Token kopieren">⧉</button>
          <button class="remove-btn" data-action="remove-player" data-player-id="${escapeHtml(playerId)}" data-player-name="${escapeHtml(name)}" title="Spieler entfernen">✕</button>
        </div>
      </div>
    `;
    container.appendChild(div);
  });

  // Update overview count
  const overviewPlayers = document.getElementById("overviewPlayers");
  if (overviewPlayers) {
    overviewPlayers.textContent = getPlayerCount();
  }
}

/**
 * Remove a player (sends WebSocket message)
 * Note: wsConn is passed from main host.js
 */
export function removePlayer(playerId, playerName, wsConn) {
  const displayName = playerName || `${playerId.substring(0, 8)}...`;
  if (
    confirm(
      `Spieler "${displayName}" entfernen?\n\n` +
        "- Der Spieler wird aus dem Spiel entfernt\n" +
        "- Seine Antwort wird gelscht (falls vorhanden)\n" +
        "- Betroffene Stimmen werden zurckgesetzt\n\n" +
        "Dies kann nicht rckgngig gemacht werden!",
    )
  ) {
    wsConn.send({
      t: "host_remove_player",
      player_id: playerId,
    });
    showAlert(`Spieler ${displayName} wird entfernt...`, "info");
  }
}
