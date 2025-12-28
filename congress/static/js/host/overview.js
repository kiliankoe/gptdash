/**
 * Host panel overview flow management
 */

import { gameState } from "./state.js";
import { getPlayerCount } from "./players.js";
import {
  renderPromptRow,
  queuePrompt,
  pendingOverviewPromptAutoQueue,
  setPendingOverviewPromptAutoQueue,
} from "./prompts.js";

// Overview action callbacks
const overviewActions = { primary: null, secondary: null };

// WebSocket connection reference (set via setWsConn)
let wsConn = null;

// Callbacks for phase transitions and other actions
let callbacks = {};

export function setWsConn(conn) {
  wsConn = conn;
}

export function setCallbacks(cbs) {
  callbacks = cbs;
}

/**
 * Run the primary overview action
 */
export function runOverviewPrimaryAction() {
  if (typeof overviewActions.primary === "function") {
    overviewActions.primary();
  }
}

/**
 * Run the secondary overview action
 */
export function runOverviewSecondaryAction() {
  if (typeof overviewActions.secondary === "function") {
    overviewActions.secondary();
  }
}

/**
 * Set overview action buttons
 */
function setOverviewActions({ primary, secondary, hint }) {
  overviewActions.primary = primary?.action ?? null;
  overviewActions.secondary = secondary?.action ?? null;

  const primaryBtn = document.getElementById("overviewPrimaryActionBtn");
  const secondaryBtn = document.getElementById("overviewSecondaryActionBtn");
  const hintEl = document.getElementById("overviewFlowHint");

  if (primaryBtn) {
    primaryBtn.textContent = primary?.label ?? "";
    primaryBtn.disabled = !(primary?.enabled ?? false);
  }
  if (secondaryBtn) {
    secondaryBtn.textContent = secondary?.label ?? "";
    secondaryBtn.disabled = !(secondary?.enabled ?? false);
    secondaryBtn.style.display = secondary?.label ? "inline-flex" : "none";
  }
  if (hintEl) {
    hintEl.textContent = hint ?? "";
  }
}

/**
 * Update overview flow based on current game phase
 */
export function updateOverviewFlow() {
  const phase = gameState.phase;
  const validTargets = gameState.validTransitions || [];
  const queuedCount = gameState.queuedPrompts.length;
  const playerCount = getPlayerCount();

  // Show/hide voting controls card based on phase
  const votingCard = document.getElementById("votingControlsCard");
  if (votingCard) {
    votingCard.style.display = phase === "VOTING" ? "block" : "none";
  }

  const canTransitionTo = (target) => validTargets.includes(target);
  const canStartPromptSelection =
    queuedCount > 0 && canTransitionTo("PROMPT_SELECTION");

  // Handle RESULTS phase with two steps
  if (phase === "RESULTS") {
    const resultsStep = gameState.resultsStep || 0;

    if (resultsStep === 0) {
      // Step 0: Breakdown - show "Leaderboards zeigen" button
      setOverviewActions({
        primary: {
          label: " Leaderboards zeigen",
          enabled: true,
          action: () => callbacks.resultsNextStep?.(),
        },
        secondary: {
          label: null,
          enabled: false,
          action: null,
        },
        hint: "Zeigt alle Antworten mit Stimmverteilung. Weiter zu Leaderboards.",
      });
    } else {
      // Step 1: Leaderboards - show "Podium" button with back option
      setOverviewActions({
        primary: {
          label: " Podium",
          enabled: canTransitionTo("PODIUM"),
          action: () => callbacks.transitionPhase?.("PODIUM"),
        },
        secondary: {
          label: " Zurck zu Stimmen",
          enabled: true,
          action: () => callbacks.resultsPrevStep?.(),
        },
        hint: "KI und lustigste Antwort aufgelst. Weiter zum Podium.",
      });
    }
    return;
  }

  if (phase === "LOBBY" || phase === "PODIUM") {
    if (playerCount === 0) {
      setOverviewActions({
        primary: {
          label: "Spieler erstellen",
          enabled: true,
          action: () => callbacks.hostCreatePlayersFromOverview?.(),
        },
        secondary: {
          label: "3 Spieler (schnell)",
          enabled: true,
          action: () => callbacks.hostCreatePlayers?.(3),
        },
        hint: "Erst Spieler erstellen, dann Prompt(s) in die Warteschlange legen.",
      });
      return;
    }

    setOverviewActions({
      primary: {
        label: queuedCount > 1 ? " Prompt-Voting starten" : " Runde starten",
        enabled: canStartPromptSelection,
        action: () => callbacks.startPromptSelection?.(),
      },
      secondary: {
        label:
          canTransitionTo("LOBBY") && phase !== "LOBBY" ? "Zur Lobby" : null,
        enabled: canTransitionTo("LOBBY") && phase !== "LOBBY",
        action: () => callbacks.transitionPhase?.("LOBBY"),
      },
      hint:
        queuedCount === 0
          ? "Fge einen Prompt hinzu und lege ihn in die Warteschlange."
          : queuedCount === 1
            ? "1 Prompt  startet direkt die Schreibphase."
            : "Mehrere Prompts  Publikum stimmt ab, Gewinner geht in Schreiben.",
    });
    return;
  }

  if (phase === "PROMPT_SELECTION") {
    setOverviewActions({
      primary: {
        label: " Schreiben starten",
        enabled: canTransitionTo("WRITING"),
        action: () => callbacks.transitionPhase?.("WRITING"),
      },
      secondary: {
        label: "Pause",
        enabled: canTransitionTo("INTERMISSION"),
        action: () => callbacks.transitionPhase?.("INTERMISSION"),
      },
      hint: "Wenn genug Votes da sind (oder du abkrzen willst), starte Schreiben.",
    });
    return;
  }

  if (phase === "WRITING") {
    setOverviewActions({
      primary: {
        label: " Antworten zeigen",
        enabled: canTransitionTo("REVEAL"),
        action: () => callbacks.closeWriting?.(),
      },
      secondary: {
        label: "+10 Sekunden",
        enabled: !!gameState.deadline,
        action: () => callbacks.extendTimer?.(10),
      },
      hint: "Warte bis alle eingereicht haben, dann Reveal starten.",
    });
    return;
  }

  if (phase === "REVEAL") {
    setOverviewActions({
      primary: {
        label: "Weiter ",
        enabled: true,
        action: () => callbacks.revealNext?.(),
      },
      secondary: {
        label: " Abstimmen",
        enabled: canTransitionTo("VOTING"),
        action: () => callbacks.transitionPhase?.("VOTING"),
      },
      hint: "Mit Weiter durch die Antworten blttern, dann Abstimmen starten.",
    });
    return;
  }

  if (phase === "VOTING") {
    setOverviewActions({
      primary: {
        label: " Auflsung",
        enabled: canTransitionTo("RESULTS"),
        action: () => callbacks.transitionPhase?.("RESULTS"),
      },
      secondary: {
        label: "+10 Sekunden",
        enabled: !!gameState.deadline,
        action: () => callbacks.extendTimer?.(10),
      },
      hint: "Voting luft; Timer wechselt automatisch zur Auflsung, falls aktiv.",
    });
    return;
  }

  if (phase === "INTERMISSION") {
    setOverviewActions({
      primary: {
        label: "Zur Lobby",
        enabled: canTransitionTo("LOBBY"),
        action: () => callbacks.transitionPhase?.("LOBBY"),
      },
      secondary: {
        label: "Spiel beenden",
        enabled: canTransitionTo("ENDED"),
        action: () => callbacks.transitionPhase?.("ENDED"),
      },
      hint: "Pause-Modus.",
    });
    return;
  }

  if (phase === "ENDED") {
    setOverviewActions({
      primary: { label: "", enabled: false, action: null },
      secondary: { label: null, enabled: false, action: null },
      hint: "Spiel beendet.",
    });
  }
}

/**
 * Update overview reveal status display
 */
export function updateOverviewRevealStatus() {
  const el = document.getElementById("overviewRevealStatus");
  if (!el) return;

  if (gameState.phase !== "REVEAL") {
    el.textContent = "";
    return;
  }

  const round = gameState.currentRound;
  const total =
    round?.reveal_order?.length ??
    (Array.isArray(gameState.revealOrder) ? gameState.revealOrder.length : 0);
  const idx = round?.reveal_index ?? 0;

  if (!total) {
    el.textContent = "Keine Antworten zum Anzeigen.";
    return;
  }

  el.textContent = `Position: ${Math.min(idx + 1, total)}/${total}`;
}

/**
 * Filter overview prompts
 */
export function filterOverviewPrompts() {
  updateOverviewPromptPool();
}

/**
 * Update overview prompt pool display
 */
export function updateOverviewPromptPool() {
  const container = document.getElementById("overviewPromptPoolList");
  if (!container) return;

  const query =
    document
      .getElementById("overviewPromptSearchInput")
      ?.value.trim()
      .toLowerCase() ?? "";

  const queuedIds = new Set(gameState.queuedPrompts.map((p) => p.id));
  const queueFull = gameState.queuedPrompts.length >= 3;

  const prompts = (gameState.prompts || [])
    .filter((p) => {
      // Only show host prompts on overview
      if (p.source !== "host") return false;
      if (!query) return true;
      const haystack = `${p.text ?? ""} ${p.image_url ?? ""}`.toLowerCase();
      return haystack.includes(query);
    })
    .sort((a, b) => {
      const at = a.created_at ?? "";
      const bt = b.created_at ?? "";
      return bt.localeCompare(at);
    })
    .slice(0, 15);

  container.innerHTML = "";

  if (prompts.length === 0) {
    container.innerHTML =
      '<p style="opacity: 0.6;">Keine passenden Prompts.</p>';
    return;
  }

  prompts.forEach((promptData) => {
    container.appendChild(renderPromptRow(promptData, queuedIds, queueFull));
  });
}

/**
 * Try to auto-queue a prompt that was just added from overview
 */
export function maybeAutoQueueOverviewPrompt() {
  if (!pendingOverviewPromptAutoQueue) return;

  const queuedIds = new Set(gameState.queuedPrompts.map((p) => p.id));
  const { text, image_url } = pendingOverviewPromptAutoQueue;

  const candidates = (gameState.prompts || [])
    .filter(
      (p) =>
        p.source === "host" &&
        !queuedIds.has(p.id) &&
        (p.text ?? null) === (text ?? null) &&
        (p.image_url ?? null) === (image_url ?? null),
    )
    .sort((a, b) => (b.created_at ?? "").localeCompare(a.created_at ?? ""));

  if (candidates.length === 0) return;

  queuePrompt(candidates[0].id, wsConn);
  setPendingOverviewPromptAutoQueue(null);
}
