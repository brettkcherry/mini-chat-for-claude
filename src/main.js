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
import { getVersion } from "@tauri-apps/api/app";
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
//
// Reads --bg-rgb / --bubble-user-rgb (defined per-theme in styles.css)
// via getComputedStyle rather than hardcoding a color here, so this
// automatically tracks whichever theme is currently active — call this
// AFTER data-theme is set on <html>, never before.
let opacityPct = parseInt(localStorage.getItem("opacity") || "92", 10);
function applyOpacity(pct) {
  opacityPct = Math.min(100, Math.max(30, pct));
  localStorage.setItem("opacity", String(opacityPct));
  const a = opacityPct / 100;
  const style = getComputedStyle(document.documentElement);
  const bgRgb = style.getPropertyValue("--bg-rgb").trim();
  const bubbleRgb = style.getPropertyValue("--bubble-user-rgb").trim();
  const root = document.documentElement.style;
  root.setProperty("--bg", `rgba(${bgRgb}, ${a})`);
  root.setProperty("--bubble-user", `rgba(${bubbleRgb}, ${Math.min(1, a + 0.15)})`);
}

// ---------- Theme (System / Light / Dark) ----------
// Default is "dark" (not "system") even for users who never touch this —
// every screenshot and the icon palette were tuned against dark, so it
// stays the baseline; light is opt-in, not a surprise switch for existing
// users whose OS happens to be in light mode.
let themeChoice = localStorage.getItem("theme") || "dark"; // "system"|"light"|"dark"
const THEME_LABELS = { system: "System", light: "Light", dark: "Dark" };
const prefersDarkMql = window.matchMedia("(prefers-color-scheme: dark)");

function resolveTheme() {
  return themeChoice === "system"
    ? (prefersDarkMql.matches ? "dark" : "light")
    : themeChoice;
}

function applyTheme() {
  document.documentElement.dataset.theme = resolveTheme();
  applyOpacity(opacityPct); // re-derive --bg/--bubble-user for the new theme
}

// Only matters while themeChoice === "system"; harmless otherwise.
prefersDarkMql.addEventListener("change", () => {
  if (themeChoice === "system") applyTheme();
});

// ---------- Model registry ----------
// The live list comes from the API at startup (see refreshModels) — the
// /v1/models endpoint reports current IDs *and* each model's exact effort
// capabilities, so the picker can't go stale and retired models drop off
// on their own.
//
// This baked-in list is only the offline fallback: shown when there's no
// key yet or the fetch fails. Verified against the docs 2026-07-30;
// newest first. Deliberately omitted: Opus 4.1 (deprecated, retires
// 2026-08-05) and Mythos 5 (invitation-only). Effort levels are canonical
// lowercase API values.
//
// No `max_tokens` here on purpose: the live list carries each model's real
// output ceiling, and a guess baked in alongside these labels would be one
// more number to keep in sync. Absent, Rust clamps to the app's own maximum.
const FALLBACK_MODELS = [
  { label: "Fable 5", id: "claude-fable-5", efforts: ["low", "medium", "high", "xhigh", "max"] },
  { label: "Opus 5", id: "claude-opus-5", efforts: ["low", "medium", "high", "xhigh", "max"] },
  { label: "Sonnet 5", id: "claude-sonnet-5", efforts: ["low", "medium", "high", "xhigh", "max"] },
  { label: "Opus 4.8", id: "claude-opus-4-8", efforts: ["low", "medium", "high", "xhigh", "max"] },
  { label: "Opus 4.7", id: "claude-opus-4-7", efforts: ["low", "medium", "high", "xhigh", "max"] },
  { label: "Sonnet 4.6", id: "claude-sonnet-4-6", efforts: ["low", "medium", "high", "max"] },
  { label: "Opus 4.6", id: "claude-opus-4-6", efforts: ["low", "medium", "high", "max"] },
  { label: "Opus 4.5", id: "claude-opus-4-5-20251101", efforts: ["low", "medium", "high"] },
  { label: "Haiku 4.5", id: "claude-haiku-4-5-20251001", efforts: [] },
  { label: "Sonnet 4.5", id: "claude-sonnet-4-5-20250929", efforts: [] },
];

let MODELS = FALLBACK_MODELS.slice();

const DEFAULT_MODEL_ID = "claude-sonnet-5"; // best speed/intelligence balance
let modelIdx = 0;

/// Select by ID, not index — the list is dynamic, so a saved index would
/// point at a different model whenever the lineup changes.
function selectModelById(id) {
  const i = MODELS.findIndex((m) => m.id === id);
  modelIdx = i >= 0 ? i : 0;
}
selectModelById(localStorage.getItem("model") || DEFAULT_MODEL_ID);

// ---------- Effort level ----------
// Stored canonically lowercase ("auto" | "low" | ... | "xhigh" | "max")
// and only capitalized for display. Keeping one canonical value avoids
// the class of bug where a cosmetic relabel silently changes what gets
// sent to the API.
const EFFORT_LABELS = {
  auto: "Auto",
  low: "Low",
  medium: "Medium",
  high: "High",
  xhigh: "xHigh",
  max: "Max",
};

// Older builds persisted capitalized values — normalize on read.
let effortChoice = (localStorage.getItem("effort") || "auto").toLowerCase();

const effortEl = document.getElementById("effort-picker");

function effortOptions() {
  return ["auto", ...(MODELS[modelIdx]?.efforts ?? [])];
}

function renderEffortBadge() {
  const supported = (MODELS[modelIdx]?.efforts ?? []).length > 0;
  effortEl.style.display = supported ? "" : "none";
  if (!supported) return;
  if (!effortOptions().includes(effortChoice)) {
    effortChoice = "auto"; // e.g. xhigh selected, then switched to Sonnet 4.6
    localStorage.setItem("effort", effortChoice);
  }
  effortEl.textContent = EFFORT_LABELS[effortChoice] ?? effortChoice;
}

effortEl.addEventListener("click", () => {
  const opts = effortOptions();
  effortChoice = opts[(opts.indexOf(effortChoice) + 1) % opts.length];
  localStorage.setItem("effort", effortChoice);
  renderEffortBadge();
});

function renderModelLabel() {
  els.modelPicker.textContent = MODELS[modelIdx]?.label ?? "—";
  renderEffortBadge(); // effort options depend on the model
}
renderModelLabel();

// Click-to-cycle worked for 4 models; with the full lineup (10+) it's a
// dropdown. Anchored to the composer so it always sits directly above,
// regardless of how tall the textarea has grown.
els.modelPicker.addEventListener("click", () => {
  if (document.querySelector(".model-menu")) {
    closeModelMenu();
  } else {
    showModelMenu();
  }
});

function closeModelMenu() {
  document.querySelector(".model-menu")?.remove();
}

function showModelMenu() {
  const menu = document.createElement("div");
  menu.className = "model-menu";
  menu.innerHTML = MODELS.map(
    (m, i) => `
      <button class="model-menu__row${i === modelIdx ? " is-current" : ""}" data-idx="${i}">
        <span class="model-menu__name">${escapeHtml(m.label)}</span>
        ${m.efforts.length === 0 ? '<span class="model-menu__note">no effort</span>' : ""}
      </button>`
  ).join("");

  menu.addEventListener("click", (e) => {
    const row = e.target.closest("[data-idx]");
    if (!row) return;
    modelIdx = parseInt(row.dataset.idx, 10);
    localStorage.setItem("model", MODELS[modelIdx].id);
    renderModelLabel();
    closeModelMenu();
  });

  document.querySelector(".composer").appendChild(menu);
  menu.querySelector(".is-current")?.scrollIntoView({ block: "nearest" });
}

/// Pull the live model list from the API. Silent no-op on failure — the
/// fallback list is already in place, and a missing key surfaces its own
/// error when the user actually sends something.
async function refreshModels() {
  try {
    const live = await invoke("list_models");
    if (!Array.isArray(live) || live.length === 0) return;
    const currentId = MODELS[modelIdx]?.id;
    MODELS = live;
    selectModelById(localStorage.getItem("model") || currentId || DEFAULT_MODEL_ID);
    renderModelLabel();
  } catch (err) {
    console.warn("model list fetch failed, using fallback:", err);
  }
}

// ---------- Window controls ----------
const appWindow = getCurrentWindow();

// Always-on-top — persisted, applied at startup, toggled from Settings.
let alwaysOnTop = localStorage.getItem("pin") === "1";
async function setAlwaysOnTop(v) {
  alwaysOnTop = v;
  localStorage.setItem("pin", v ? "1" : "0");
  await appWindow.setAlwaysOnTop(v);
}

// Close-to-tray — defaults ON (opt-out, not opt-in): a hidden app with no
// taskbar entry and no tray icon, recoverable only via a memorized
// keyboard shortcut, is a discoverability trap for anyone but a power
// user. Persisted, synced to Rust at startup and on toggle. Rust (tray.rs)
// owns the actual hide/show + icon lifecycle so the "window open XOR tray
// icon present" invariant holds no matter which entry point triggered it
// (close button, global shortcut, tray click).
let closeToTray = localStorage.getItem("closeToTray") !== "0";
async function setCloseToTray(v) {
  closeToTray = v;
  localStorage.setItem("closeToTray", v ? "1" : "0");
  await invoke("set_close_to_tray", { enabled: v }).catch(() => {});
}

els.min.addEventListener("click", () => appWindow.minimize());

els.close.addEventListener("click", async (e) => {
  // Plain click: tray-hide if "close to tray" is on, else a full quit —
  // decided in Rust (handle_close), matching whatever's actually stored
  // there rather than trusting this tab's local copy. Shift+click always
  // force-quits regardless of the setting, as a fast escape hatch.
  if (e.shiftKey) {
    await invoke("quit_app");
  } else {
    await invoke("handle_close");
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
// One button, two jobs: send while idle, stop while a reply is streaming.
// Same slot means no layout shift mid-turn and no second control to explain.
els.send.addEventListener("click", () => {
  if (inFlight) cancelChat();
  else submit();
});

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
  // At high effort a reply can take 30s+ before the first token. Without
  // this, silence is indistinguishable from a hang.
  announce("Claude is replying");

  try {
    await invoke("send_chat", {
      model: MODELS[modelIdx].id,
      messages: history.slice(),
      // effortChoice is already the canonical lowercase API value.
      effort:
        effortChoice === "auto" || (MODELS[modelIdx]?.efforts ?? []).length === 0
          ? null
          : effortChoice,
      // The selected model's own reported output ceiling, from /v1/models.
      // Null on the offline fallback list, which is fine — Rust clamps a
      // missing value to the app's maximum rather than guessing low.
      maxTokens: MODELS[modelIdx]?.max_tokens ?? null,
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
    announce(`Error. ${err}`);
  } finally {
    setComposerBusy(false);
  }
}

/// Speak something to a screen reader.
///
/// The transcript itself deliberately isn't a live region — the streaming
/// bubble re-renders on every token, so marking it live would read a growing
/// partial sentence hundreds of times per reply. Announcements are curated
/// here instead: that a reply started, and the finished text once it's done.
///
/// Re-setting textContent to the same string wouldn't re-announce, so the
/// field is cleared first — a real quirk of live regions, not superstition.
function announce(text) {
  const region = document.getElementById("sr-status");
  if (!region) return;
  region.textContent = "";
  // A tick's delay so the clear and the set land as two separate mutations.
  setTimeout(() => {
    region.textContent = text;
  }, 50);
}

// Icons for the send/stop button. Square-ish stop mark — the universal
// "halt" shape, and it reads clearly at 14px where a thinner glyph wouldn't.
const SEND_ICON = `<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
    <path fill="currentColor" d="M1.724 1.053a.5.5 0 0 0-.714.545l1.403 4.85a.5.5 0 0 0 .397.354l5.69.953c.268.053.268.437 0 .49l-5.69.953a.5.5 0 0 0-.397.354l-1.403 4.85a.5.5 0 0 0 .714.545l13-6.5a.5.5 0 0 0 0-.894l-13-6.5Z"/>
  </svg>`;
const STOP_ICON = `<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
    <rect x="4" y="4" width="8" height="8" rx="1.5" fill="currentColor"/>
  </svg>`;

function setComposerBusy(busy) {
  inFlight = busy;
  els.input.disabled = busy;
  // The send button stays enabled and becomes the stop control — disabling it
  // (as this used to) is what left a long reply with no way out but waiting.
  els.send.disabled = false;
  els.send.innerHTML = busy ? STOP_ICON : SEND_ICON;
  els.send.classList.toggle("composer__send--stop", busy);
  els.send.title = busy ? "Stop generating (Esc)" : "Send (Enter)";
  els.send.setAttribute("aria-label", busy ? "Stop generating" : "Send message");
}

/// Halt the turn in flight. The partial reply stays on screen and gets
/// committed to history by the stop event Rust sends back, so a stopped
/// answer is still part of the conversation rather than being thrown away.
async function cancelChat() {
  if (!inFlight) return;
  try {
    await invoke("cancel_chat");
  } catch (err) {
    console.warn("cancel failed:", err);
  }
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
  const { delta, stop, notice } = event.payload;
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
    //
    // This runs for every ending, not just clean ones — a stop, a mid-stream
    // API error, a dropped connection. Rust guarantees exactly one stop event
    // per turn precisely so a partial reply is never stranded on screen but
    // absent from the conversation the next turn is built from.
    const committed = streamingRaw;
    if (committed) history.push({ role: "assistant", content: committed });

    // Why the turn ended, when that isn't "it finished": cut off at the
    // length cap, declined, stopped, connection dropped. Absent on a normal
    // completion — a note on every reply would train people to ignore them.
    if (notice) appendNotice(streamingBubble, notice);

    // A refusal, or an error before the first token, leaves the bubble empty.
    // The notice already says what happened — an empty shell next to it just
    // looks broken.
    if (!streamingRaw) streamingBubble.remove();

    streamingBubble.classList.remove("msg--streaming");

    // The whole reply, read once now that it's settled. Notices are appended
    // so the reason a turn ended early is spoken too, not just shown.
    const spoken = [committed, notice].filter(Boolean).join(". ");
    if (spoken) announce(spoken);

    streamingBubble = null;
    streamingRaw = "";
    autosaveSession();
    scrollToBottom();
  }
});

/// Attach a one-line explanation under a reply. Plain text via textContent —
/// this is app copy today, but it sits next to a bubble that renders model
/// output, and the two should never differ in how carefully they're handled.
function appendNotice(bubble, text) {
  const note = document.createElement("div");
  note.className = "msg__notice";
  note.setAttribute("role", "status");
  note.textContent = text;
  bubble.insertAdjacentElement("afterend", note);
}

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
  const modelMenu = document.querySelector(".model-menu");
  if (
    modelMenu &&
    !modelMenu.contains(e.target) &&
    !els.modelPicker.contains(e.target)
  ) {
    closeModelMenu();
  }
});

// Escape closes the model dropdown (it's the only true popover), and
// otherwise stops generation — the muscle memory from every other chat UI.
// Dismissing the popover wins when one is open, so a single Esc never both
// closes a menu and kills a reply the user still wanted.
document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape") return;
  if (document.querySelector(".model-menu")) {
    closeModelMenu();
    return;
  }
  if (inFlight) {
    e.preventDefault();
    cancelChat();
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
      // ids are escaped like any other untrusted value. The backend already
      // drops non-alphanumeric ids from the listing, so this should never have
      // anything to do — it's here so that the guarantee doesn't rest on a
      // single check in another language.
      const id = escapeHtml(m.id);
      return `
        <div class="sessions__row${current}" data-id="${id}">
          <div class="sessions__rowmain">
            <div class="sessions__rowtitle">${escapeHtml(m.title)}</div>
            <div class="sessions__rowmeta">${when} · ${m.messageCount} messages</div>
          </div>
          <button class="sessions__del" data-del="${id}" title="Delete session">✕</button>
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
    <button class="sessions__danger" data-act="delete-all">Delete all chat history</button>
    <button class="sessions__quit" data-act="quit">Quit Mini Chat for Claude</button>
  `;

  const status = card.querySelector(".sessions__status");

  // "Delete all" is irreversible and sits one stray click away from the
  // per-session ✕ buttons, so it arms on the first click and only fires on the
  // second. Disarms itself after a few seconds so it can't stay hot.
  const DELETE_ALL_LABEL = "Delete all chat history";
  let armTimer = null;
  function disarmDeleteAll() {
    const btn = card.querySelector('[data-act="delete-all"]');
    if (!btn) return;
    clearTimeout(armTimer);
    armTimer = null;
    delete btn.dataset.armed;
    btn.classList.remove("is-armed");
    btn.textContent = DELETE_ALL_LABEL;
  }

  card.addEventListener("click", async (e) => {
    const act = e.target.closest("[data-act]")?.dataset.act;
    const delBtn = e.target.closest("[data-del]");
    const row = e.target.closest(".sessions__row");

    // Any click that isn't the armed button itself cancels the pending wipe.
    if (act !== "delete-all") disarmDeleteAll();

    if (act === "delete-all") {
      const btn = e.target.closest('[data-act="delete-all"]');
      if (btn.dataset.armed !== "1") {
        btn.dataset.armed = "1";
        btn.classList.add("is-armed");
        btn.textContent = "Click again to permanently delete every saved chat";
        armTimer = setTimeout(disarmDeleteAll, 5000);
        return;
      }
      try {
        const removed = await invoke("delete_all_sessions");
        card.querySelector(".sessions__list").innerHTML =
          '<div class="sessions__empty">No saved sessions yet.</div>';
        status.textContent =
          removed === 1 ? "✓ Deleted 1 saved chat" : `✓ Deleted ${removed} saved chats`;
      } catch (err) {
        status.textContent = String(err);
      }
      disarmDeleteAll();
      return;
    }

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
    if (delBtn) {
      e.stopPropagation();
      try {
        await invoke("delete_session", { id: delBtn.dataset.del });
        // Walk up from the button that was clicked rather than building a
        // selector out of the id — no interpolation, nothing to escape.
        delBtn.closest(".sessions__row")?.remove();
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

// ---------- App version ----------
// Fetched once in init() (getVersion() reads tauri.conf.json's "version"
// field, baked in at build time — no Rust command needed). Empty until
// then; appVersionLabel() degrades gracefully either way.
let appVersion = "";

function appVersionLabel() {
  return appVersion ? `Mini Chat for Claude v${appVersion}` : "Mini Chat for Claude";
}

// ---------- Easter egg: ocean accent ----------
// The entire UI hangs off one accent colour, so swapping it to the icon's
// teal is two CSS variables. All the thought went into the trigger.
//
// Deliberately hidden rather than made a Settings row: a visible "accent
// colour" control invites "add more colours", which is precisely the scope
// creep CONTRIBUTING warns about. Hidden, it costs nothing and never grows a
// backlog. If it turns out people genuinely want the teal to match the icon,
// promoting this to a real setting later is easy — the reverse isn't.
const ACCENT_KEY = "accentOcean";
let oceanAccent = localStorage.getItem(ACCENT_KEY) === "1";

function applyAccent() {
  if (oceanAccent) document.documentElement.dataset.accent = "ocean";
  else delete document.documentElement.dataset.accent;
}
applyAccent();

// Five clicks on the version line — the same gesture as tapping an Android
// build number, which is the point: it's the spot a curious person already
// pokes at. The counter resets after a pause so stray clicks across separate
// visits to Settings can't quietly accumulate into a surprise.
let versionClicks = 0;
let versionResetTimer = null;

function nudgeVersionCounter(el) {
  clearTimeout(versionResetTimer);
  versionResetTimer = setTimeout(() => {
    versionClicks = 0;
  }, 2000);

  if (++versionClicks < 5) return;
  versionClicks = 0;

  oceanAccent = !oceanAccent;
  localStorage.setItem(ACCENT_KEY, oceanAccent ? "1" : "0");
  applyAccent();

  // The whole UI changing colour is its own confirmation. This only names
  // what happened, so it reads as intentional rather than as a glitch — and
  // tells you the gesture that undoes it.
  el.textContent = oceanAccent ? "🌊 ocean — five more to undo" : "🔸 warm";
  setTimeout(() => {
    el.textContent = appVersionLabel();
  }, 1600);
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
    <div class="settings-card__row" data-set="tray">
      <span>Close to tray</span>
      <span class="settings-card__toggle ${closeToTray ? "is-on" : ""}"></span>
    </div>
    <div class="settings-card__row" data-set="raw">
      <span>Raw text mode <span class="settings-card__hint">&lt;/&gt;</span></span>
      <span class="settings-card__toggle ${rawMode ? "is-on" : ""}"></span>
    </div>
    <div class="settings-card__row" data-set="theme">
      <span>Theme</span>
      <span class="settings-card__pill">${THEME_LABELS[themeChoice]}</span>
    </div>
    <div class="settings-card__row settings-card__row--slider">
      <span>Opacity</span>
      <input type="range" min="30" max="100" step="1" value="${opacityPct}" class="settings-card__slider" />
      <span class="settings-card__pct">${opacityPct}%</span>
    </div>
    <button class="settings-card__keybtn">API key…</button>
    <div class="settings-card__version">${escapeHtml(appVersionLabel())}</div>
    <div class="settings-card__disclaimer">Unofficial · not affiliated with Anthropic</div>
  `;

  card
    .querySelector(".settings-card__version")
    .addEventListener("click", (ev) => nudgeVersionCounter(ev.currentTarget));

  card.querySelector('[data-set="pin"]').addEventListener("click", (ev) => {
    // Flip the switch immediately (optimistic update) — don't wait on the
    // invoke() round trip first. Previously this awaited setAlwaysOnTop()
    // before touching the DOM, so the toggle only ever visually updated
    // on the NEXT render (i.e. reopening Settings), never on the click
    // itself.
    const next = !alwaysOnTop;
    ev.currentTarget
      .querySelector(".settings-card__toggle")
      .classList.toggle("is-on", next);
    setAlwaysOnTop(next);
  });

  card.querySelector('[data-set="tray"]').addEventListener("click", (ev) => {
    const next = !closeToTray;
    ev.currentTarget
      .querySelector(".settings-card__toggle")
      .classList.toggle("is-on", next);
    setCloseToTray(next);
  });

  card.querySelector('[data-set="raw"]').addEventListener("click", (ev) => {
    rawMode = !rawMode;
    localStorage.setItem("rawMode", rawMode ? "1" : "0");
    applyRawMode();
    ev.currentTarget
      .querySelector(".settings-card__toggle")
      .classList.toggle("is-on", rawMode);
  });

  card.querySelector('[data-set="theme"]').addEventListener("click", (ev) => {
    const order = ["system", "light", "dark"];
    themeChoice = order[(order.indexOf(themeChoice) + 1) % order.length];
    localStorage.setItem("theme", themeChoice);
    applyTheme();
    ev.currentTarget.querySelector(".settings-card__pill").textContent =
      THEME_LABELS[themeChoice];
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
  applyTheme(); // sets data-theme, then calls applyOpacity for us
  if (alwaysOnTop) {
    appWindow.setAlwaysOnTop(true).catch(() => {});
  }
  // Always sync, not just when true: Rust's own default is also "on" now,
  // so the one case that actually needs telling is a saved "off".
  invoke("set_close_to_tray", { enabled: closeToTray }).catch(() => {});
  try {
    appVersion = await getVersion();
  } catch {
    // stays "" — appVersionLabel() degrades gracefully
  }
  // Not awaited — the fallback list is already usable, so don't make
  // startup wait on a network round-trip.
  if (hasKey) refreshModels();
}
init();
