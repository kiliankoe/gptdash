/**
 * Host panel state management
 */

// Central game state object
export const gameState = {
  phase: "LOBBY",
  roundNo: 0,
  players: [], // Legacy: just tokens
  playerStatus: [], // New: full player status with names
  submissions: [],
  revealOrder: [], // Current reveal order (submission IDs)
  prompts: [], // Prompt candidates from audience (filtered by shadowban)
  promptStats: {
    total: 0,
    host_count: 0,
    audience_count: 0,
    top_submitters: [],
  }, // Stats about prompt pool
  queuedPrompts: [], // Prompts queued for next round (max 3)
  scores: { players: [], audience_top: [] },
  validTransitions: [], // Populated by server
  panicMode: false,
  deadline: null,
  selectedAiSubmissionId: null, // Currently selected AI submission
  aiGenerationStatus: "idle", // idle, generating, completed, failed
  currentRound: null, // Current round info
  currentPrompt: null, // Currently selected prompt for the round
};

// Prompt section collapse state (persisted in localStorage)
export const promptSectionState = {
  hostPrompts:
    localStorage.getItem("promptSection_hostPrompts") !== "collapsed",
  audiencePrompts:
    localStorage.getItem("promptSection_audiencePrompts") !== "collapsed",
};

// Drag-and-drop state
export const dragState = {
  draggedId: null,
  draggedElement: null,
};

/**
 * Reset UI state when round changes
 */
export function resetRoundUiState(callbacks) {
  gameState.submissions = [];
  gameState.revealOrder = [];
  gameState.selectedAiSubmissionId = null;
  gameState.aiGenerationStatus = "idle";

  // Call update callbacks if provided
  if (callbacks) {
    callbacks.updateSubmissionsList?.();
    callbacks.updatePanicModeUI?.();
    callbacks.updateOverviewFlow?.();
    callbacks.updateOverviewRevealStatus?.();
  }
}
