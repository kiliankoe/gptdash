/**
 * Common JavaScript utilities for GPTDash clients
 */

/**
 * WebSocket connection manager
 */
class WSConnection {
  constructor(role, onMessage, onStatusChange, token = null) {
    this.role = role;
    this.onMessage = onMessage;
    this.onStatusChange = onStatusChange;
    this.token = token;
    this.ws = null;
    this.reconnectDelay = 2000;
  }

  setToken(token) {
    this.token = token;
  }

  /**
   * Force reconnect with current token
   * Useful when token changes and we need to re-validate
   */
  reconnect() {
    if (this.ws) {
      // Prevent auto-reconnect from firing
      this.ws.onclose = null;
      this.ws.close();
    }
    this.connect();
  }

  connect() {
    const protocol = window.location.protocol === "https:" ? "wss" : "ws";
    const host = window.location.host || window.location.hostname;
    let wsUrl = `${protocol}://${host}/ws?role=${this.role}`;
    if (this.token) {
      wsUrl += `&token=${encodeURIComponent(this.token)}`;
    }
    console.log("Connecting to:", wsUrl);

    this.ws = new WebSocket(wsUrl);

    this.ws.onopen = () => {
      console.log("Connected to game server");
      if (this.onStatusChange) {
        this.onStatusChange(true, "Connected");
      }
    };

    this.ws.onmessage = (event) => {
      const message = JSON.parse(event.data);
      console.log("Received:", message);
      if (this.onMessage) {
        this.onMessage(message);
      }
    };

    this.ws.onerror = (error) => {
      console.error("WebSocket error:", error);
      if (this.onStatusChange) {
        this.onStatusChange(false, "Connection error");
      }
    };

    this.ws.onclose = () => {
      console.log("Disconnected from game server");
      if (this.onStatusChange) {
        this.onStatusChange(false, "disconnected");
      }
      // Auto-reconnect
      setTimeout(() => this.connect(), this.reconnectDelay);
    };
  }

  send(message) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(message));
      console.log("Sent:", message);
      return true;
    } else {
      console.error("WebSocket not connected");
      return false;
    }
  }

  isConnected() {
    return this.ws && this.ws.readyState === WebSocket.OPEN;
  }
}

/**
 * Update connection status UI
 */
function updateConnectionStatus(connected, text) {
  const statusEl = document.getElementById("connectionStatus");
  if (statusEl) {
    statusEl.className = `connection-status ${connected ? "connected" : "disconnected"}`;
    statusEl.textContent =
      text || (connected ? "Verbunden" : "Nicht verbunden");
  }

  const dotEl = document.getElementById("statusDot");
  if (dotEl) {
    if (connected) {
      dotEl.classList.add("connected");
    } else {
      dotEl.classList.remove("connected");
    }
  }

  const textEl = document.getElementById("statusText");
  if (textEl) {
    textEl.textContent = text || (connected ? "Verbunden" : "Nicht verbunden");
  }
}

/**
 * Show/hide screens
 */
function showScreen(screenId) {
  // Hide all screens
  document.querySelectorAll(".screen").forEach((screen) => {
    screen.classList.remove("active");
  });

  // Show target screen
  const screen = document.getElementById(screenId);
  if (screen) {
    screen.classList.add("active");
  } else {
    console.warn("Screen not found:", screenId);
  }
}

/**
 * Show/hide panels (for multi-panel layouts)
 * @param {string} panelId - ID of the panel to show
 * @param {boolean} updateUrl - Whether to update the URL search params (default: true)
 */
function showPanel(panelId, updateUrl = true) {
  // Hide all panels
  document.querySelectorAll(".panel").forEach((panel) => {
    panel.classList.remove("active");
  });

  // Show target panel
  const panel = document.getElementById(panelId);
  if (panel) {
    panel.classList.add("active");
  } else {
    console.warn("Panel not found:", panelId);
  }

  // Update sidebar navigation if it exists
  document.querySelectorAll(".sidebar-item").forEach((item) => {
    item.classList.remove("active");
  });

  // Find and activate the corresponding sidebar item
  document.querySelectorAll(".sidebar-item").forEach((item) => {
    const onclick = item.getAttribute("onclick");
    if (onclick?.includes(`showPanel('${panelId}')`)) {
      item.classList.add("active");
    }
  });

  // Update URL search params for persistence across reloads
  if (updateUrl && panel) {
    const url = new URL(window.location);
    url.searchParams.set("panel", panelId);
    window.history.replaceState({}, "", url);
  }
}

/**
 * Restore panel from URL search params (call on page load)
 * @returns {boolean} true if a panel was restored from URL
 */
function restorePanelFromUrl() {
  const url = new URL(window.location);
  const panelId = url.searchParams.get("panel");
  if (panelId) {
    const panel = document.getElementById(panelId);
    if (panel) {
      showPanel(panelId, false); // Don't update URL again
      return true;
    }
  }
  return false;
}

/**
 * Error message display
 */
function showError(elementId, message) {
  const errorEl = document.getElementById(elementId);
  if (errorEl) {
    errorEl.textContent = message;
    errorEl.style.display = "block";
  }
}

function hideError(elementId) {
  const errorEl = document.getElementById(elementId);
  if (errorEl) {
    errorEl.style.display = "none";
  }
}

/**
 * Escape HTML to prevent XSS
 */
function escapeHtml(text) {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

/**
 * Generate a random ID
 */
function generateId(prefix = "") {
  const timestamp = Date.now().toString(36);
  const random = Math.random().toString(36).substring(2);
  return prefix ? `${prefix}_${timestamp}_${random}` : `${timestamp}_${random}`;
}

/**
 * Copy text to clipboard
 */
function copyToClipboard(text) {
  if (navigator.clipboard) {
    navigator.clipboard
      .writeText(text)
      .then(() => {
        console.log("Copied to clipboard:", text);
        return true;
      })
      .catch((err) => {
        console.error("Failed to copy:", err);
        return false;
      });
  } else {
    // Fallback for older browsers
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.style.position = "fixed";
    textarea.style.opacity = "0";
    document.body.appendChild(textarea);
    textarea.select();
    try {
      document.execCommand("copy");
      console.log("Copied to clipboard:", text);
      return true;
    } catch (err) {
      console.error("Failed to copy:", err);
      return false;
    } finally {
      document.body.removeChild(textarea);
    }
  }
}

/**
 * Format timestamp
 */
function formatTime(date = new Date()) {
  return date.toLocaleTimeString();
}

/**
 * Debounce a function
 * @param {Function} fn - Function to debounce
 * @param {number} delay - Delay in milliseconds
 */
function debounce(fn, delay) {
  let timeoutId;
  return function (...args) {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn.apply(this, args), delay);
  };
}

/**
 * Timer countdown manager
 */
class CountdownTimer {
  constructor(elementId, onComplete) {
    this.elementId = elementId;
    this.onComplete = onComplete;
    this.interval = null;
    this.deadline = null;
    this.serverTimeOffset = 0; // Difference between server time and local time
  }

  /**
   * Start countdown to a deadline
   * @param {string} deadlineISO - ISO8601 timestamp
   * @param {string} serverNowISO - Server's current time (for clock sync)
   */
  start(deadlineISO, serverNowISO) {
    this.stop(); // Clear any existing timer

    if (!deadlineISO) {
      this.hide();
      return;
    }

    // Calculate server time offset
    const serverNow = new Date(serverNowISO);
    const localNow = new Date();
    this.serverTimeOffset = serverNow.getTime() - localNow.getTime();

    this.deadline = new Date(deadlineISO);
    this.update();
    this.interval = setInterval(() => this.update(), 100); // Update 10x per second
  }

  stop() {
    if (this.interval) {
      clearInterval(this.interval);
      this.interval = null;
    }
  }

  /**
   * Change the target element for the countdown display
   * @param {string} elementId - New element ID to update
   */
  setElement(elementId) {
    this.elementId = elementId;
  }

  hide() {
    const el = document.getElementById(this.elementId);
    if (el) {
      el.textContent = "";
      el.style.display = "none";
    }
  }

  update() {
    const el = document.getElementById(this.elementId);
    if (!el || !this.deadline) return;

    // Use server-synchronized time
    const now = new Date(Date.now() + this.serverTimeOffset);
    const remaining = this.deadline.getTime() - now.getTime();

    if (remaining <= 0) {
      el.textContent = "00:00";
      el.style.display = "block";
      this.stop();
      if (this.onComplete) {
        this.onComplete();
      }
      return;
    }

    const totalSeconds = Math.ceil(remaining / 1000);
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;

    el.textContent = `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
    el.style.display = "block";
  }

  /**
   * Update deadline from server (e.g., when extended)
   * @param {string} deadlineISO - New deadline ISO8601 timestamp
   * @param {string} serverNowISO - Server's current time (for clock sync)
   */
  updateDeadline(deadlineISO, serverNowISO) {
    if (!deadlineISO) {
      this.hide();
      return;
    }

    // Update server time offset
    const serverNow = new Date(serverNowISO);
    const localNow = new Date();
    this.serverTimeOffset = serverNow.getTime() - localNow.getTime();

    // Update deadline
    this.deadline = new Date(deadlineISO);

    // If timer was stopped, restart it
    if (!this.interval) {
      this.update();
      this.interval = setInterval(() => this.update(), 100);
    }
  }
}

/**
 * QR Code generation utility
 */
const QRCodeManager = {
  /**
   * Generate a QR code for a URL
   * @param {string} containerId - ID of the element to render QR code in
   * @param {string} url - URL to encode in QR code
   * @param {object} options - QR code options (width, height, colorDark, colorLight)
   */
  generate(containerId, url, options = {}) {
    const container = document.getElementById(containerId);
    if (!container) {
      console.error("QR code container not found:", containerId);
      return;
    }

    // Clear existing QR code
    container.innerHTML = "";

    // Check if QRCode library is loaded
    if (typeof QRCode === "undefined") {
      console.error("QRCode library not loaded");
      container.innerHTML =
        '<p style="color: red;">QR code library not loaded</p>';
      return;
    }

    const defaultOptions = {
      width: 256,
      height: 256,
      colorDark: "#000000",
      colorLight: "#ffffff",
    };

    const qrOptions = { ...defaultOptions, ...options };

    try {
      new QRCode(container, {
        text: url,
        width: qrOptions.width,
        height: qrOptions.height,
        colorDark: qrOptions.colorDark,
        colorLight: qrOptions.colorLight,
        correctLevel: QRCode.CorrectLevel.H, // High error correction
      });
    } catch (error) {
      console.error("Failed to generate QR code:", error);
      container.innerHTML =
        '<p style="color: red;">Failed to generate QR code</p>';
    }
  },

  /**
   * Get the full URL for the audience join page
   */
  getAudienceJoinUrl() {
    const protocol = window.location.protocol;
    const host = window.location.host || window.location.hostname;
    return `${protocol}//${host}/`;
  },

  /**
   * Get the full URL for the player join page (with optional token)
   */
  getPlayerJoinUrl(token = null) {
    const protocol = window.location.protocol;
    const host = window.location.host || window.location.hostname;
    const baseUrl = `${protocol}//${host}/player.html`;
    return token ? `${baseUrl}?token=${token}` : baseUrl;
  },

  /**
   * Generate QR code for audience join
   */
  generateAudienceQR(containerId, options = {}) {
    const url = this.getAudienceJoinUrl();
    this.generate(containerId, url, options);
    return url;
  },

  /**
   * Generate QR code for player join
   */
  generatePlayerQR(containerId, token = null, options = {}) {
    const url = this.getPlayerJoinUrl(token);
    this.generate(containerId, url, options);
    return url;
  },
};

/**
 * Text-to-Speech manager using browser API
 * Includes workarounds for Chrome bugs (stuck synthesis, 15-second cutoff)
 */
class TTSManager {
  constructor() {
    this.synthesis = window.speechSynthesis;
    this.currentUtterance = null;
    this.voicesLoaded = false;
    this.resumeInterval = null;

    // Wait for voices to load (Chrome loads them asynchronously)
    if (this.synthesis) {
      // Try to get voices immediately (Firefox)
      if (this.synthesis.getVoices().length > 0) {
        this.voicesLoaded = true;
      }
      // Listen for async voice loading (Chrome/Edge)
      this.synthesis.addEventListener("voiceschanged", () => {
        this.voicesLoaded = true;
        console.log(
          "[TTS] Voices loaded:",
          this.synthesis.getVoices().length,
          "voices available",
        );
      });
    }
  }

  /**
   * Speak text using browser TTS
   * @param {string} text - Text to speak
   * @param {object} options - TTS options (rate, pitch, volume, voice, onEnd, onError)
   */
  speak(text, options = {}) {
    if (!this.synthesis) {
      console.warn("[TTS] Speech synthesis not supported in this browser");
      return;
    }

    if (!text || text.trim().length === 0) {
      console.warn("[TTS] Empty text, skipping");
      return;
    }

    // Cancel any ongoing speech
    this.stop();

    const utterance = new SpeechSynthesisUtterance(text);
    utterance.lang = options.lang || "de-DE";
    utterance.rate = options.rate || 1.0;
    utterance.pitch = options.pitch || 1.0;
    utterance.volume = options.volume || 1.0;

    // Select voice if specified and voices are loaded
    if (options.voiceName && this.voicesLoaded) {
      const voices = this.synthesis.getVoices();
      const voice = voices.find((v) => v.name === options.voiceName);
      if (voice) {
        utterance.voice = voice;
      }
    }

    // Add event handlers for debugging and error detection
    utterance.onstart = () => {
      console.log("[TTS] Speech started");
    };

    utterance.onend = () => {
      console.log("[TTS] Speech ended");
      this._stopResumeWorkaround();
      if (options.onEnd) options.onEnd();
    };

    utterance.onerror = (event) => {
      console.error("[TTS] Speech error:", event.error);
      this._stopResumeWorkaround();
      if (options.onError) options.onError(event.error);
    };

    this.currentUtterance = utterance;

    // Chrome bug workaround: need a small delay after cancel()
    // Otherwise the new utterance may not play
    setTimeout(() => {
      this.synthesis.speak(utterance);

      // Chrome bug workaround: utterances longer than ~15 seconds get cut off
      // Periodically calling pause/resume prevents this
      this._startResumeWorkaround();
    }, 50);
  }

  /**
   * Chrome workaround: pause/resume every 10 seconds to prevent cutoff
   * See: https://bugs.chromium.org/p/chromium/issues/detail?id=679437
   */
  _startResumeWorkaround() {
    this._stopResumeWorkaround();
    this.resumeInterval = setInterval(() => {
      if (this.synthesis?.speaking && !this.synthesis.paused) {
        this.synthesis.pause();
        this.synthesis.resume();
      }
    }, 10000);
  }

  _stopResumeWorkaround() {
    if (this.resumeInterval) {
      clearInterval(this.resumeInterval);
      this.resumeInterval = null;
    }
  }

  /**
   * Stop any ongoing speech
   */
  stop() {
    this._stopResumeWorkaround();
    if (this.synthesis) {
      this.synthesis.cancel();
    }
    this.currentUtterance = null;
  }

  /**
   * Check if currently speaking
   */
  isSpeaking() {
    return this.synthesis?.speaking;
  }
}

/**
 * Vote challenge solver for anti-automation
 * Computes SHA256(nonce + voter_token) and returns first 16 hex chars
 */
class ChallengeSolver {
  constructor() {
    this.currentNonce = null;
    this.currentRoundId = null;
  }

  /**
   * Store challenge from server
   * @param {string} nonce - Challenge nonce from server
   * @param {string} roundId - Round ID for validation
   */
  setChallenge(nonce, roundId) {
    this.currentNonce = nonce;
    this.currentRoundId = roundId;
    console.log("Challenge received:", {
      nonce: `${nonce?.slice(0, 8)}...`,
      roundId,
    });
  }

  /**
   * Check if a challenge is available
   */
  hasChallenge() {
    return this.currentNonce !== null;
  }

  /**
   * Clear the current challenge
   */
  clear() {
    this.currentNonce = null;
    this.currentRoundId = null;
  }

  /**
   * Solve the current challenge
   * @param {string} voterToken - The voter's token
   * @returns {Promise<{nonce: string, response: string}>}
   */
  async solve(voterToken) {
    if (!this.currentNonce) {
      throw new Error("No challenge set");
    }

    const input = this.currentNonce + voterToken;
    const hash = await this.sha256(input);
    const response = hash.slice(0, 16); // First 16 hex chars (8 bytes)

    return {
      nonce: this.currentNonce,
      response: response,
    };
  }

  /**
   * Compute SHA-256 hash using Web Crypto API
   * @param {string} message - Input message
   * @returns {Promise<string>} Hex-encoded hash
   */
  async sha256(message) {
    const msgBuffer = new TextEncoder().encode(message);
    const hashBuffer = await crypto.subtle.digest("SHA-256", msgBuffer);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    return hashArray.map((b) => b.toString(16).padStart(2, "0")).join("");
  }
}

/**
 * Handle phase change with round detection
 * Shared utility for detecting round changes and updating phase state
 * @param {object} msg - Phase change message from server
 * @param {object} state - State object with phase and roundNo properties
 * @param {function} onRoundChange - Callback when round number changes
 */
function handlePhaseChange(msg, state, onRoundChange) {
  if (
    typeof msg.round_no === "number" &&
    state.roundNo !== null &&
    msg.round_no !== state.roundNo
  ) {
    onRoundChange();
  }
  state.phase = msg.phase;
  if (typeof msg.round_no === "number") {
    state.roundNo = msg.round_no;
  }
}

/**
 * Render prompt display with text and/or image
 * Hides text element for image-only prompts
 * @param {object} prompt - Prompt object with text and image_url properties
 * @param {HTMLElement} textEl - Text display element
 * @param {HTMLElement} imageEl - Image container element
 */
function renderPromptDisplay(prompt, textEl, imageEl) {
  if (textEl) {
    textEl.textContent = prompt?.text || "";
    textEl.style.display = prompt?.text ? "block" : "none";
  }
  if (imageEl) {
    if (prompt?.image_url) {
      imageEl.innerHTML = `<img src="${escapeHtml(prompt.image_url)}" alt="Prompt image" class="prompt-image-display">`;
      imageEl.style.display = "block";
    } else {
      imageEl.innerHTML = "";
      imageEl.style.display = "none";
    }
  }
}

/**
 * Get display name from an entity with fallback
 * @param {object} entity - Object with display_name or ref_id property
 * @param {number} fallbackLength - Length of ref_id substring for fallback (default: 12)
 * @returns {string} Display name
 */
function getDisplayName(entity, fallbackLength = 12) {
  return (
    entity.display_name ||
    entity.ref_id?.substring(0, fallbackLength) ||
    "Unknown"
  );
}

/**
 * Message dispatcher for WebSocket messages
 * Provides a registry-based approach to handle messages by type
 */
class MessageDispatcher {
  constructor(role) {
    this.role = role;
    this.handlers = new Map();
  }

  /**
   * Register a handler for a message type
   * @param {string} messageType - Message type to handle (msg.t)
   * @param {function} handler - Handler function
   * @returns {MessageDispatcher} this for chaining
   */
  on(messageType, handler) {
    this.handlers.set(messageType, handler);
    return this;
  }

  /**
   * Dispatch a message to the appropriate handler
   * @param {object} msg - WebSocket message with .t property
   */
  dispatch(msg) {
    const handler = this.handlers.get(msg.t);
    if (handler) {
      handler(msg);
    } else {
      console.warn(`[${this.role}] Unhandled message type: ${msg.t}`);
    }
  }
}

// ES6 module exports (for host.js and its modules)
export {
  WSConnection,
  updateConnectionStatus,
  showScreen,
  showPanel,
  restorePanelFromUrl,
  showError,
  hideError,
  escapeHtml,
  generateId,
  copyToClipboard,
  formatTime,
  debounce,
  CountdownTimer,
  QRCodeManager,
  TTSManager,
  ChallengeSolver,
  handlePhaseChange,
  renderPromptDisplay,
  getDisplayName,
  MessageDispatcher,
};

// Also expose on window for non-module scripts (beamer, player, audience)
if (typeof window !== "undefined") {
  Object.assign(window, {
    WSConnection,
    updateConnectionStatus,
    showScreen,
    showPanel,
    restorePanelFromUrl,
    showError,
    hideError,
    escapeHtml,
    generateId,
    copyToClipboard,
    formatTime,
    debounce,
    CountdownTimer,
    QRCodeManager,
    TTSManager,
    ChallengeSolver,
    handlePhaseChange,
    renderPromptDisplay,
    getDisplayName,
    MessageDispatcher,
  });
}
