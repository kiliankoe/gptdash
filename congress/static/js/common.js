/**
 * Common JavaScript utilities for GPTDash clients
 */

/**
 * WebSocket connection manager
 */
class WSConnection {
  constructor(role, onMessage, onStatusChange) {
    this.role = role;
    this.onMessage = onMessage;
    this.onStatusChange = onStatusChange;
    this.ws = null;
    this.reconnectDelay = 2000;
  }

  connect() {
    const protocol = window.location.protocol === "https:" ? "wss" : "ws";
    const host = window.location.host || window.location.hostname;
    const wsUrl = `${protocol}://${host}/ws?role=${this.role}`;
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
        this.onStatusChange(false, "Disconnected");
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
 */
function showPanel(panelId) {
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
}

/**
 * Text-to-Speech manager using browser API
 */
class TTSManager {
  constructor() {
    this.synthesis = window.speechSynthesis;
    this.currentUtterance = null;
  }

  /**
   * Speak text using browser TTS
   * @param {string} text - Text to speak
   * @param {object} options - TTS options (rate, pitch, volume, voice)
   */
  speak(text, options = {}) {
    if (!this.synthesis) {
      console.warn("Speech synthesis not supported in this browser");
      return;
    }

    // Cancel any ongoing speech
    this.stop();

    const utterance = new SpeechSynthesisUtterance(text);
    utterance.rate = options.rate || 1.0;
    utterance.pitch = options.pitch || 1.0;
    utterance.volume = options.volume || 1.0;

    // Select voice if specified
    if (options.voiceName) {
      const voices = this.synthesis.getVoices();
      const voice = voices.find((v) => v.name === options.voiceName);
      if (voice) {
        utterance.voice = voice;
      }
    }

    this.currentUtterance = utterance;
    this.synthesis.speak(utterance);
  }

  /**
   * Stop any ongoing speech
   */
  stop() {
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

if (typeof window !== "undefined") {
  Object.assign(window, {
    WSConnection,
    updateConnectionStatus,
    showScreen,
    showPanel,
    showError,
    hideError,
    escapeHtml,
    generateId,
    copyToClipboard,
    formatTime,
    CountdownTimer,
    TTSManager,
  });
}
