// Claude Mini — frontend bootstrap
//
// Architecture:
// - JS owns the conversation history (just an array of {role, content}).
// - On submit: append user msg, invoke('send_chat') with the full history,
//   open a streaming assistant bubble.
// - Rust streams the Anthropic response and emits `chat-chunk` events.
// - We append each delta into the open bubble; on stop, commit to history.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { marked } from "marked";
import DOMPurify from "dompurify";

// Chat-style markdown: single newlines become <br>, GFM tables/strikethrough.
marked.setOptions({ breaks: true, gfm: true });

/** Model output → sanitized HTML. Never feed raw model text to innerHTML. */
function renderMarkdown(text) {
  return DOMPurify.sanitize(marked.parse(text));
}

// ---------- DOM ----------
const els = {
  app: document.getElementById("app"),
  messages: document.getElementById("messages"),
  input: document.getElementById("input"),
  send: document.getElementById("send"),
  close: document.getElementById("btn-close"),
  min: document.getElementById("btn-min"),
  settings: document.getElementById("btn-settings"),
  newChat: document.getElementById("btn-new"),
  history: document.getElementById("btn-history"),
  modelPicker: document.getElementById("model-picker"),
};

// ---------- Render mode (markdown vs bare-bones raw text) ----------
// Persisted in localStorage so the choice survives restarts. The raw
// markdown source is kept on each bubble in dataset.raw, so toggling
// re-renders every existing message either way — nothing is lost.
let rawMode = localStorage.getItem("rawMode") === "1";

function renderAssistantNode(node) {
  const raw = node.dataset.raw ?? "";
  if (rawMode) {
    node.textContent = raw;
  } else {
    node.innerHTML = renderMarkdown(raw);
  }
}

function applyRawMode() {
  els.app.classList.toggle("raw-mode", rawMode);
  document
    .querySelectorAll(".msg--claude:not(.msg--error)")
    .forEach(renderAssistantNode);
}

// ---------- Background opacity ----------
// Fades the window chrome, keeps text fully readable (background alpha
// only, not whole-window opacity). Persisted; applied at startup.
let opacityPct = parseInt(localStorage.getItem("opacity") || "92", 10);
function applyOpacity(pct) {
  opacityPct = Math.min(100, Math.max(30, pct));
  localStorage.setItem("opacity", String(opacityPct));
  const a = opacityPct / 100;
  const root = document.documentElement.style;
  root.setProperty("--bg", `rgba(20, 20, 22, ${a})`);
  root.setProperty("--bubble-user", `rgba(42, 42, 46, ${Math.min(1, a + 0.15)})`);
}

// ---------- Model registry ----------
// IDs and effort support verified against the live /v1/models response
// (June 2026). Haiku doesn't support effort at all; Sonnet lacks xhigh.
// Future-proofing idea: query /v1/models at startup instead of hardcoding.
const MODELS = [
  { label: "Fable 5", id: "claude-fable-5", efforts: ["low", "medium", "high", "xhigh", "max"] },
  { label: "Opus 4.8", id: "claude-opus-4-8", efforts: ["low", "medium", "high", "xhigh", "max"] },
  { label: "Sonnet 4.6", id: "claude-sonnet-4-6", efforts: ["low", "medium", "high", "max"] },
  { label: "Haiku 4.5", id: "claude-haiku-4-5-20251001", efforts: [] },
];
let modelIdx = 2; // default: Sonnet — best speed/cost balance for a widget

// ---------- Effort level ----------
// "auto" = don't send the field; the API picks its default. Persisted.
let effortChoice = localStorage.getItem("effort") || "auto";

const effortEl = document.getElementById("effort-picker");

function effortOptions() {
  return ["auto", ...MODELS[modelIdx].efforts];
}

function renderEffortBadge() {
  const supported = MODELS[modelIdx].efforts.length > 0;
  effortEl.style.display = supported ? "" : "none";
  if (!supported) return;
  if (!effortOptions().includes(effortChoice)) {
    effortChoice = "auto"; // e.g. xhigh selected, then switched to Sonnet
    localStorage.setItem("effort", effortChoice);
  }
  effortEl.textContent = effortChoice;
}

effortEl.addEventListener("click", () => {
  const opts = effortOptions();
  effortChoice = opts[(opts.indexOf(effortChoice) + 1) % opts.length];
  localStorage.setItem("effort", effortChoice);
  renderEffortBadge();
});

function renderModelLabel() {
  els.modelPicker.textContent = MODELS[modelIdx].label;
  renderEffortBadge(); // effort options depend on the model
}
renderModelLabel();

els.modelPicker.addEventListener("click", () => {
  modelIdx = (modelIdx + 1) % MODELS.length;
  renderModelLabel();
});

// ---------- Window controls ----------
const appWindow = getCurrentWindow();

// Always-on-top — persisted, applied at startup, toggled from Settings.
let alwaysOnTop = localStorage.getItem("pin") === "1";
async function setAlwaysOnTop(v) {
  alwaysOnTop = v;
  localStorage.setItem("pin", v ? "1" : "0");
  await appWindow.setAlwaysOnTop(v);
}

els.min.addEventListener("click", () => appWindow.minimize());

els.close.addEventListener("click", async (e) => {
  // Plain click hides (summon back with Ctrl+Shift+Space).
  // Shift+click fully quits the app.
  if (e.shiftKey) {
    await invoke("quit_app");
  } else {
    await appWindow.hide();
  }
});

// ---------- Composer ----------
function autosize() {
  els.input.style.height = "auto";
  els.input.style.height = Math.min(els.input.scrollHeight, 160) + "px";
}
els.input.addEventListener("input", autosize);

els.input.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
    e.preventDefault();
    submit();
  }
});
els.send.addEventListener("click", submit);

// ---------- Conversation state ----------
let history = []; // [{ role: 'user'|'assistant', content: string }]
let inFlight = false; // single-request-at-a-time guard
let streamingBubble = null; // the live assistant DOM node currently filling
let streamingRaw = ""; // raw markdown accumulated for the streaming bubble

// ---------- Session state ----------
// Every chat is a session, autosaved to disk after each message. Nothing
// is ever silently lost: "new chat" just starts a fresh file.
let sessionId = newSessionId();
let sessionCreated = Date.now();

function newSessionId() {
  return "s" + Date.now();
}

function sessionTitle() {
  const first = history.find((m) => m.role === "user");
  if (!first) return "Untitled chat";
  const t = first.content.replace(/\s+/g, " ").trim();
  return t.length > 48 ? t.slice(0, 48) + "…" : t;
}

async function autosaveSession() {
  if (history.length === 0) return;
  try {
    await invoke("save_session", {
      session: {
        id: sessionId,
        title: sessionTitle(),
        createdMs: sessionCreated,
        updatedMs: Date.now(),
        model: MODELS[modelIdx].id,
        messages: history,
      },
    });
  } catch (err) {
    console.error("session autosave failed:", err);
  }
}

const WELCOME_HTML = `
  <div class="welcome">
    <div class="welcome__title">Mini Chat for Claude</div>
    <div class="welcome__hint">Type below to start a conversation.</div>
    <div class="welcome__hint welcome__hint--kbd">Ctrl+Shift+Space summons or hides this window</div>
  </div>`;

function startNewChat() {
  if (inFlight) return; // don't yank the rug mid-stream
  // Current session is already autosaved after every message — just reset.
  history = [];
  sessionId = newSessionId();
  sessionCreated = Date.now();
  els.messages.innerHTML = WELCOME_HTML;
  els.input.focus();
}

async function submit() {
  if (inFlight) return;
  const text = els.input.value.trim();
  if (!text) return;

  history.push({ role: "user", content: text });
  appendMessage("user", text);
  els.input.value = "";
  autosize();
  autosaveSession();

  setComposerBusy(true);
  streamingBubble = beginStreamingBubble();
  streamingRaw = "";

  try {
    await invoke("send_chat", {
      model: MODELS[modelIdx].id,
      messages: history.slice(),
      effort:
        effortChoice === "auto" || MODELS[modelIdx].efforts.length === 0
          ? null
          : effortChoice,
    });
    // No-op here: history commit happens in the 'stop' chunk handler so
    // we capture the final text from the bubble's accumulated content.
  } catch (err) {
    // Replace the streaming bubble with an error message so it's obvious.
    if (streamingBubble) {
      streamingBubble.classList.add("msg--error");
      streamingBubble.textContent = String(err);
      streamingBubble = null;
    } else {
      appendMessage("error", String(err));
    }
  } finally {
    setComposerBusy(false);
  }
}

function setComposerBusy(busy) {
  inFlight = busy;
  els.send.disabled = busy;
  els.input.disabled = busy;
}

// ---------- Messages ----------
function clearWelcome() {
  const welcome = els.messages.querySelector(".welcome");
  if (welcome) welcome.remove();
}

function appendMessage(role, text) {
  clearWelcome();
  const div = document.createElement("div");
  div.className = `msg msg--${role}`;
  div.textContent = text;
  els.messages.appendChild(div);
  scrollToBottom();
  return div;
}

function beginStreamingBubble() {
  clearWelcome();
  const div = document.createElement("div");
  div.className = "msg msg--claude msg--streaming";
  div.textContent = "";
  els.messages.appendChild(div);
  scrollToBottom();
  return div;
}

function scrollToBottom() {
  els.messages.scrollTop = els.messages.scrollHeight;
}

// ---------- Streaming events from Rust ----------
// Note: not awaited — we don't need the unlisten handle, and top-level
// await breaks the production build target.
listen("chat-chunk", (event) => {
  const { delta, stop } = event.payload;
  if (!streamingBubble) return;

  if (delta) {
    // Re-render the accumulated markdown on every chunk so formatting
    // (bold, lists, code blocks) appears live as it streams in.
    streamingRaw += delta;
    streamingBubble.dataset.raw = streamingRaw;
    renderAssistantNode(streamingBubble);
    scrollToBottom();
  }

  if (stop) {
    // Commit the RAW markdown to history (not rendered HTML) so the
    // conversation context sent back to the API stays clean.
    history.push({ role: "assistant", content: streamingRaw });
    streamingBubble.classList.remove("msg--streaming");
    streamingBubble = null;
    streamingRaw = "";
    autosaveSession();
  }
});

// Links inside rendered markdown must NOT navigate the webview away from
// the app (that would replace the whole UI with the linked page). Instead:
// click copies the URL to the clipboard with a fading "Copied" signal.
els.messages.addEventListener("click", async (e) => {
  const a = e.target.closest("a");
  if (!a) return;
  e.preventDefault();
  try {
    await navigator.clipboard.writeText(a.href);
    showCopiedTag(a);
  } catch {
    // Clipboard unavailable — at least surface the URL.
    a.title = a.href;
  }
});

function showCopiedTag(anchor) {
  // One tag per link at a time.
  anchor.nextElementSibling?.classList.contains("copied-tag") &&
    anchor.nextElementSibling.remove();
  const tag = document.createElement("span");
  tag.className = "copied-tag";
  tag.textContent = "Copied";
  anchor.insertAdjacentElement("afterend", tag);
  tag.addEventListener("animationend", () => tag.remove());
}

// Click anywhere outside a floating card dismisses it — except the key
// card during first-run (no key, composer disabled), when it's the only
// path forward and shouldn't vanish under a stray click.
document.addEventListener("click", (e) => {
  const setup = els.messages.querySelector(".setup");
  if (setup && !setup.contains(e.target) && !els.input.disabled) {
    setup.remove();
  }
  const sessions = els.messages.querySelector(".sessions");
  if (
    sessions &&
    !sessions.contains(e.target) &&
    !els.history.contains(e.target)
  ) {
    sessions.remove();
  }
  const settings = els.messages.querySelector(".settings-card");
  if (
    settings &&
    !settings.contains(e.target) &&
    !els.settings.contains(e.target)
  ) {
    settings.remove();
  }
});

// ---------- New chat + sessions browser + export ----------
els.newChat.addEventListener("click", startNewChat);

els.history.addEventListener("click", () => {
  const existing = els.messages.querySelector(".sessions");
  if (existing) {
    existing.remove();
  } else {
    showSessionsCard();
  }
});

function transcriptMarkdown() {
  const lines = [
    `# Mini Chat for Claude — ${sessionTitle()}`,
    `_Exported ${new Date().toLocaleString()} · model: ${MODELS[modelIdx].label}_`,
    "",
  ];
  for (const m of history) {
    lines.push(m.role === "user" ? "**You:**" : "**Claude:**", "", m.content, "", "---", "");
  }
  return lines.join("\n");
}

async function showSessionsCard() {
  els.messages.querySelector(".sessions")?.remove();

  let metas = [];
  try {
    metas = await invoke("list_sessions");
  } catch (err) {
    console.error("list_sessions failed:", err);
  }

  const card = document.createElement("div");
  card.className = "sessions";

  const rows = metas
    .map((m) => {
      const when = new Date(m.updatedMs).toLocaleString(undefined, {
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      });
      const current = m.id === sessionId ? " sessions__row--current" : "";
      return `
        <div class="sessions__row${current}" data-id="${m.id}">
          <div class="sessions__rowmain">
            <div class="sessions__rowtitle">${escapeHtml(m.title)}</div>
            <div class="sessions__rowmeta">${when} · ${m.messageCount} messages</div>
          </div>
          <button class="sessions__del" data-del="${m.id}" title="Delete session">✕</button>
        </div>`;
    })
    .join("");

  card.innerHTML = `
    <div class="sessions__title">Sessions</div>
    <div class="sessions__actions">
      <button class="sessions__btn" data-act="copy">Copy chat as Markdown</button>
      <button class="sessions__btn" data-act="file">Save chat as .md file</button>
    </div>
    <div class="sessions__status"></div>
    <div class="sessions__list">${rows || '<div class="sessions__empty">No saved sessions yet.</div>'}</div>
    <button class="sessions__quit" data-act="quit">Quit Mini Chat for Claude</button>
  `;

  const status = card.querySelector(".sessions__status");

  card.addEventListener("click", async (e) => {
    const act = e.target.closest("[data-act]")?.dataset.act;
    const del = e.target.closest("[data-del]")?.dataset.del;
    const row = e.target.closest(".sessions__row");

    if (act === "quit") {
      await invoke("quit_app");
      return;
    }
    if (act === "copy") {
      if (history.length === 0) {
        status.textContent = "Nothing to copy yet — this chat is empty.";
        return;
      }
      await navigator.clipboard.writeText(transcriptMarkdown());
      status.textContent = "✓ Copied to clipboard";
      return;
    }
    if (act === "file") {
      if (history.length === 0) {
        status.textContent = "Nothing to export yet — this chat is empty.";
        return;
      }
      try {
        const path = await invoke("export_chat", {
          markdown: transcriptMarkdown(),
          title: sessionTitle(),
        });
        status.textContent = `✓ Saved: ${path}`;
      } catch (err) {
        status.textContent = String(err);
      }
      return;
    }
    if (del) {
      e.stopPropagation();
      try {
        await invoke("delete_session", { id: del });
        card.querySelector(`.sessions__row[data-id="${del}"]`)?.remove();
        if (!card.querySelector(".sessions__row")) {
          card.querySelector(".sessions__list").innerHTML =
            '<div class="sessions__empty">No saved sessions yet.</div>';
        }
      } catch (err) {
        status.textContent = String(err);
      }
      return;
    }
    if (row) {
      await loadSession(row.dataset.id);
      card.remove();
    }
  });

  els.messages.prepend(card);
}

async function loadSession(id) {
  if (inFlight) return;
  let s;
  try {
    s = await invoke("load_session", { id });
  } catch (err) {
    console.error("load_session failed:", err);
    return;
  }
  history = s.messages.slice();
  sessionId = s.id;
  sessionCreated = s.createdMs;
  const idx = MODELS.findIndex((m) => m.id === s.model);
  if (idx >= 0) {
    modelIdx = idx;
    renderModelLabel();
  }
  // Rebuild the message DOM from history.
  els.messages.innerHTML = "";
  for (const m of history) {
    if (m.role === "user") {
      appendMessage("user", m.content);
    } else {
      const div = document.createElement("div");
      div.className = "msg msg--claude";
      div.dataset.raw = m.content;
      renderAssistantNode(div);
      els.messages.appendChild(div);
    }
  }
  scrollToBottom();
  els.input.focus();
}

function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[c]);
}

// ---------- Settings ----------
els.settings.addEventListener("click", () => {
  const existing = els.messages.querySelector(".settings-card");
  if (existing) {
    existing.remove();
  } else {
    showSettingsCard();
  }
});

function showSettingsCard() {
  els.messages.querySelector(".settings-card")?.remove();

  const card = document.createElement("div");
  card.className = "settings-card";
  card.innerHTML = `
    <div class="settings-card__title">Settings</div>
    <div class="settings-card__row" data-set="pin">
      <span>Always on top</span>
      <span class="settings-card__toggle ${alwaysOnTop ? "is-on" : ""}"></span>
    </div>
    <div class="settings-card__row" data-set="raw">
      <span>Raw text mode <span class="settings-card__hint">&lt;/&gt;</span></span>
      <span class="settings-card__toggle ${rawMode ? "is-on" : ""}"></span>
    </div>
    <div class="settings-card__row settings-card__row--slider">
      <span>Opacity</span>
      <input type="range" min="30" max="100" step="1" value="${opacityPct}" class="settings-card__slider" />
      <span class="settings-card__pct">${opacityPct}%</span>
    </div>
    <button class="settings-card__keybtn">API key…</button>
  `;

  card.querySelector('[data-set="pin"]').addEventListener("click", async (ev) => {
    await setAlwaysOnTop(!alwaysOnTop);
    ev.currentTarget
      .querySelector(".settings-card__toggle")
      .classList.toggle("is-on", alwaysOnTop);
  });

  card.querySelector('[data-set="raw"]').addEventListener("click", (ev) => {
    rawMode = !rawMode;
    localStorage.setItem("rawMode", rawMode ? "1" : "0");
    applyRawMode();
    ev.currentTarget
      .querySelector(".settings-card__toggle")
      .classList.toggle("is-on", rawMode);
  });

  const slider = card.querySelector(".settings-card__slider");
  const pctEl = card.querySelector(".settings-card__pct");
  slider.addEventListener("input", () => {
    applyOpacity(parseInt(slider.value, 10));
    pctEl.textContent = `${opacityPct}%`;
  });

  card.querySelector(".settings-card__keybtn").addEventListener("click", (ev) => {
    // stopPropagation so the document-level dismiss handler doesn't
    // instantly close the setup card we're about to open.
    ev.stopPropagation();
    card.remove();
    showSetupCard();
  });

  els.messages.prepend(card);
}

function setComposerEnabled(enabled) {
  els.input.disabled = !enabled;
  els.send.disabled = !enabled;
  els.input.placeholder = enabled
    ? "Message Claude…"
    : "Add your API key above to start";
}

async function showSetupCard() {
  // Only one card at a time.
  els.messages.querySelector(".setup")?.remove();

  let ks = { stored: false, suffix: null, env_fallback: false };
  try {
    ks = await invoke("api_key_status");
  } catch {
    /* backend unavailable — show the card anyway */
  }

  const stateLine = ks.stored
    ? `✓ A key is saved (ends in …${ks.suffix})`
    : ks.env_fallback
      ? "No key saved — currently riding the terminal's env var"
      : "No key saved yet";

  const card = document.createElement("div");
  card.className = "setup";
  card.innerHTML = `
    <div class="setup__title">Anthropic API key</div>
    <div class="setup__state ${ks.stored ? "setup__state--ok" : ""}">${stateLine}</div>
    <div class="setup__hint">
      Stored in Windows Credential Manager — never written to disk in plaintext.
      Get a key at console.anthropic.com.
    </div>
    <div class="setup__row">
      <input class="setup__input" type="password"
        placeholder="${ks.stored ? "Paste a new key to replace it…" : "sk-ant-…"}"
        spellcheck="false" />
      <button class="setup__save">Save</button>
    </div>
    <div class="setup__status"></div>
    ${ks.stored ? `<button class="setup__remove">Remove saved key</button>` : ""}
  `;

  const input = card.querySelector(".setup__input");
  const save = card.querySelector(".setup__save");
  const status = card.querySelector(".setup__status");
  const state = card.querySelector(".setup__state");
  const remove = card.querySelector(".setup__remove");

  async function doSave() {
    const key = input.value;
    if (!key.trim()) return;
    save.disabled = true;
    status.textContent = "Saving…";
    status.className = "setup__status";
    try {
      await invoke("save_api_key", { key });
      const fresh = await invoke("api_key_status").catch(() => null);
      const suffix = fresh?.suffix ? ` — ends in …${fresh.suffix}` : "";
      status.textContent = `✓ Key saved to Credential Manager${suffix}`;
      status.classList.add("setup__status--ok");
      state.textContent = `✓ A key is saved${suffix ? ` (ends in …${fresh.suffix})` : ""}`;
      state.classList.add("setup__state--ok");
      input.value = "";
      setComposerEnabled(true);
      // Linger long enough to actually read the confirmation.
      setTimeout(() => {
        card.remove();
        els.input.focus();
      }, 2200);
    } catch (err) {
      status.textContent = String(err);
      status.classList.add("setup__status--err");
      save.disabled = false;
    }
  }

  save.addEventListener("click", doSave);
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") doSave();
  });

  remove?.addEventListener("click", async () => {
    try {
      await invoke("delete_api_key");
      // Re-open the card to reflect the new state.
      showSetupCard();
    } catch (err) {
      status.textContent = String(err);
      status.classList.add("setup__status--err");
    }
  });

  els.messages.prepend(card);
  input.focus();
}

// ---------- Auto-update banner ----------
// Rust checks GitHub on startup; if a newer release exists we get this
// event. Nothing installs without the user clicking.
listen("update-available", (event) => {
  if (document.querySelector(".update-banner")) return;
  const v = event.payload?.version ?? "";
  const banner = document.createElement("button");
  banner.className = "update-banner";
  banner.textContent = `Update ${v} available — click to install & restart`;
  banner.addEventListener("click", async () => {
    banner.disabled = true;
    banner.textContent = "Updating…";
    try {
      await invoke("install_update");
      // App restarts on success; this line only runs on failure paths.
    } catch (err) {
      banner.textContent = `Update failed: ${err}`;
      banner.disabled = false;
    }
  });
  els.app.insertBefore(banner, els.messages.nextSibling);
});

// ---------- Init ----------
async function init() {
  let hasKey = false;
  try {
    hasKey = await invoke("has_api_key");
  } catch {
    // Backend unavailable — leave composer enabled; errors surface on send.
    hasKey = true;
  }
  if (!hasKey) {
    setComposerEnabled(false);
    showSetupCard();
  } else {
    els.input.focus();
  }
  applyRawMode();
  applyOpacity(opacityPct);
  if (alwaysOnTop) {
    appWindow.setAlwaysOnTop(true).catch(() => {});
  }
}
init();
