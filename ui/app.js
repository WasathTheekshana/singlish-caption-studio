"use strict";
// All text from the backend is inserted with textContent, never as HTML.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (id) => document.getElementById(id);

const state = { projects: [], selected: null, info: null, running: false, consent: false, inspecting: false, keySet: false, fonts: [], sel: new Set(), selInit: false };

function el(tag, cls, text) {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined) n.textContent = text;
  return n;
}
const msg = (e) => (typeof e === "string" ? e : "Something went wrong.");
const fmtDur = (s) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
const fmtTime = (s) => `${Math.floor(s / 60)}:${(s % 60).toFixed(1).padStart(4, "0")}`;
const fmtDate = (us) => (us ? new Date(us / 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" }) : "");
function note(id, text, kind) {
  const n = $(id);
  n.textContent = text;
  n.className = `notice${kind ? " " + kind : ""}`;
  n.hidden = !text;
}
const show = (id, on) => ($(id).hidden = !on);

// ------------------------------------------------------------------ fonts & preview (shared)
function fillFonts(select, fonts, current) {
  select.replaceChildren();
  for (const f of fonts) {
    const o = el("option", "", f.collection ? `${f.name} (collection)` : f.name);
    o.value = f.name;
    select.append(o);
  }
  const match = fonts.find((f) => f.name.toLowerCase() === (current || "").toLowerCase());
  if (match) select.value = match.name;
}
const isCollection = (name) => (state.fonts.find((f) => f.name === name) || {}).collection === true;
function paintPreview(ids, v) {
  const set = (id, font, size, bold) => {
    const n = $(id);
    n.style.fontFamily = `"${font}", var(--si)`;
    n.style.fontSize = `${size * 1.5}px`;
    n.style.fontWeight = bold ? "700" : "400";
    n.style.color = v.color;
  };
  set(ids[0], v.siFont, v.siSize, false);
  set(ids[2], v.siFont, v.siSize, false);
  set(ids[1], v.enFont, v.enSize, v.bold);
}
const COLLECTION_WARN = "A font marked “collection” may not be applied by CapCut. If captions look wrong, pick a different font.";

// ------------------------------------------------------------------ boot
async function boot() {
  let st;
  try {
    st = await invoke("setup_state");
  } catch (e) {
    show("view-main", true);
    $("project-empty").textContent = msg(e);
    show("project-empty", true);
    return;
  }
  state.keySet = st.key_set;
  if (!st.setup_done || !st.key_set) startWizard(st);
  else startMain();
}

// ------------------------------------------------------------------ wizard
const wz = { step: 1 };
function wzGo(n) {
  wz.step = n;
  for (let i = 1; i <= 3; i++) show(`wz-${i}`, i === n);
  document.querySelectorAll(".wiz-steps li").forEach((li) => {
    const i = +li.dataset.wz;
    li.className = i === n ? "active" : i < n ? "done" : "";
  });
}
function wzCapcut(st) {
  const c = st.capcut;
  let text, kind;
  if (!c.installed) { text = "CapCut was not found on this PC. Install CapCut for Windows and open it once, then press “Check again”."; kind = "bad"; }
  else if (c.tested) { text = `CapCut ${c.version} found. This version is supported.`; kind = "ok"; }
  else { text = `CapCut ${c.version || "(unknown version)"} found. This version has not been tested, so captions may not appear. A backup is made before every change.`; kind = "warn"; }
  note("wz-capcut-msg", text, kind);
  $("wz-next-1").disabled = !c.installed;
}
async function startWizard(st) {
  show("view-wizard", true);
  show("view-main", false);
  wzCapcut(st);
  state.keySet = st.key_set;
  $("wz-next-2").disabled = !st.key_set;
  if (st.key_set) note("wz-key-msg", "A key is already saved. You can continue, or paste a new one.", "ok");
  wzGo(1);
}
$("wz-recheck").addEventListener("click", async () => { try { wzCapcut(await invoke("setup_state")); } catch (e) { note("wz-capcut-msg", msg(e), "bad"); } });
$("wz-next-1").addEventListener("click", () => wzGo(2));
$("wz-open-keys").addEventListener("click", async () => {
  try { await invoke("open_link", { which: "elevenlabs_keys" }); }
  catch (e) { note("wz-key-msg", msg(e), "warn"); }
});
async function saveKey(inputId, msgId, onOk) {
  const input = $(inputId);
  const key = input.value.trim();
  if (!key) { note(msgId, "Paste your key first.", "bad"); return; }
  note(msgId, "Checking the key with ElevenLabs...", "");
  try {
    await invoke("save_api_key", { key });
    input.value = "";
    state.keySet = true;
    note(msgId, "Key accepted and saved.", "ok");
    onOk();
  } catch (e) {
    note(msgId, msg(e), "bad");
  }
}
$("wz-save-key").addEventListener("click", () => saveKey("wz-key", "wz-key-msg", async () => { $("wz-next-2").disabled = false; }));
$("wz-next-2").addEventListener("click", async () => { await wzLoadStyle(); wzGo(3); });

function wzPreview() {
  paintPreview(["wz-pv-si", "wz-pv-en", "wz-pv-si2"], { siFont: $("wz-si-font").value, siSize: +$("wz-si-size").value, enFont: $("wz-en-font").value, enSize: +$("wz-en-size").value, bold: true, color: "#ffffff" });
  $("wz-si-out").textContent = $("wz-si-size").value;
  $("wz-en-out").textContent = $("wz-en-size").value;
  const warn = isCollection($("wz-si-font").value) || isCollection($("wz-en-font").value);
  note("wz-font-warn", warn ? COLLECTION_WARN : "", "warn");
}
async function wzLoadStyle() {
  try {
    const [fonts, s] = await Promise.all([invoke("list_fonts"), invoke("get_settings")]);
    state.fonts = fonts;
    fillFonts($("wz-si-font"), fonts, s.sinhala.font);
    fillFonts($("wz-en-font"), fonts, s.english.font);
    $("wz-si-size").value = s.sinhala.size;
    $("wz-en-size").value = s.english.size;
    state.wizardSettings = s;
    wzPreview();
  } catch (e) { note("wz-style-msg", msg(e), "bad"); }
}
["wz-si-font", "wz-en-font", "wz-si-size", "wz-en-size"].forEach((id) => $(id).addEventListener("input", wzPreview));
$("wz-finish").addEventListener("click", async () => {
  const s = state.wizardSettings;
  const settings = {
    sinhala: { font: $("wz-si-font").value, size: +$("wz-si-size").value, bold: false },
    english: { font: $("wz-en-font").value, size: +$("wz-en-size").value, bold: s.english.bold },
    color: s.color, position_y: s.position_y,
  };
  try {
    await invoke("save_settings", { settings });
    await invoke("complete_setup");
    show("view-wizard", false);
    startMain();
  } catch (e) { note("wz-style-msg", msg(e), "bad"); }
});

// ------------------------------------------------------------------ main screen
function startMain() {
  show("view-main", true);
  show("view-wizard", false);
  try { state.consent = localStorage.getItem("consent-elevenlabs") === "1"; } catch { /* storage unavailable */ }
  $("consent").checked = state.consent;
  loadProjects();
  updateButton();
  startupUpdateCheck();
}
$("consent").addEventListener("change", (e) => {
  state.consent = e.target.checked;
  try { localStorage.setItem("consent-elevenlabs", state.consent ? "1" : "0"); } catch { /* ignore */ }
  updateButton();
});

async function loadProjects() {
  try { state.projects = await invoke("list_projects"); }
  catch (e) { $("project-empty").textContent = msg(e); show("project-empty", true); return; }
  show("project-empty", state.projects.length === 0);
  const list = $("project-list");
  list.replaceChildren();
  for (const p of state.projects) {
    const b = el("button", "item");
    b.type = "button";
    b.setAttribute("role", "option");
    b.setAttribute("aria-selected", String(p.folder === state.selected));
    b.append(el("span", "t", p.title), el("span", "d", fmtDate(p.modified)));
    b.addEventListener("click", () => selectProject(p.folder));
    list.append(b);
  }
}
async function selectProject(folder) {
  if (state.running) return;
  state.selected = folder;
  state.info = null;
  state.sel = new Set();
  state.selInit = false;
  resetProgress();
  note("restore-msg", "", "");
  document.querySelectorAll(".item").forEach((n, i) => n.setAttribute("aria-selected", String(state.projects[i].folder === folder)));
  await refreshInfo();
}
async function refreshInfo() {
  if (!state.selected || state.inspecting) return;
  state.inspecting = true;
  const folder = state.selected;
  try {
    const info = await invoke("inspect_project", { folder });
    if (folder === state.selected) state.info = info;
  } catch (e) {
    if (folder === state.selected) state.info = { error: msg(e), problems: [], capcut_running: false };
  } finally { state.inspecting = false; }
  renderDetails();
  updateButton();
}
function selectedSeconds() {
  const clips = (state.info && state.info.clips) || [];
  return clips.filter((c) => state.sel.has(c.id)).reduce((t, c) => t + c.audio_s, 0);
}
function renderDetails() {
  const i = state.info;
  show("details", !!i);
  if (!i) return;
  const clips = i.clips || [];
  if (!state.selInit && !i.error) {
    state.sel = new Set(clips.filter((c) => c.selected).map((c) => c.id));
    state.selInit = true;
  }
  const ids = new Set(clips.map((c) => c.id));
  state.sel = new Set([...state.sel].filter((id) => ids.has(id)));
  $("video-name").textContent = i.error ? "" : `${clips.length} clip${clips.length === 1 ? "" : "s"} with sound`;
  $("video-dur").textContent = i.duration_s ? fmtDur(i.duration_s) : "";
  const chips = $("chips");
  chips.replaceChildren();
  if (!i.error) {
    chips.append(i.capcut_running ? el("span", "chip bad", "CapCut is open") : el("span", "chip ok", "CapCut closed"));
    chips.append(i.has_captions ? el("span", "chip warn", "Has captions") : el("span", "chip", "No captions yet"));
  }
  const ul = $("clips");
  ul.replaceChildren();
  for (const c of clips) {
    const li = el("li", state.sel.has(c.id) ? "" : "off");
    const lab = el("label");
    const cb = el("input");
    cb.type = "checkbox";
    cb.checked = state.sel.has(c.id);
    cb.disabled = !c.ok || state.running;
    cb.addEventListener("change", () => {
      if (cb.checked) state.sel.add(c.id); else state.sel.delete(c.id);
      li.className = cb.checked ? "" : "off";
      updateSummary();
      updateButton();
    });
    const nm = el("span", "nm", c.name || "clip");
    if (!c.ok) nm.append(el("span", "tag", "file missing"));
    else if (c.volume < 0.5) nm.append(el("span", "tag", `quiet ${Math.round(c.volume * 100)}%`));
    lab.append(cb, nm, el("span", "tm", `${fmtDur(c.start)} to ${fmtDur(c.start + c.dur)}`));
    li.append(lab);
    ul.append(li);
  }
  show("clips", clips.length > 0);
  document.querySelector(".clip-tools").hidden = clips.length === 0;
  updateSummary();
  note("warn", (i.warnings || []).join(" "), "warn");
  note("problem", i.error || (i.problems && i.problems[0]) || "", "warn");
  show("restore-row", !!i.has_captions && !i.error);
}
function updateSummary() {
  const n = state.sel.size, total = ((state.info && state.info.clips) || []).length;
  $("clip-sum").textContent = n ? `${n} of ${total} selected, about ${Math.max(1, Math.round(selectedSeconds()))} s of audio will be transcribed` : "None selected";
}
function setAllClips(on) {
  const clips = (state.info && state.info.clips) || [];
  state.sel = new Set(on ? clips.filter((c) => c.ok).map((c) => c.id) : []);
  renderDetails();
  updateButton();
}
$("clips-all").addEventListener("click", () => setAllClips(true));
$("clips-none").addEventListener("click", () => setAllClips(false));
function updateButton() {
  const b = $("go"), i = state.info;
  let label = "Generate captions", hint = "", ok = false;
  if (state.running) label = "Working...";
  else if (!state.keySet) { label = "Add your ElevenLabs key"; hint = "Captions need your ElevenLabs key."; ok = true; }
  else if (!state.selected) label = "Choose a project";
  else if (!i) label = "Reading project...";
  else if (i.error || (i.problems && i.problems.length)) label = "Can't caption this project";
  else if (state.sel.size === 0) { label = "Tick the clips to caption"; hint = "Choose the clips that contain speech."; }
  else if (i.capcut_running) { label = "Close CapCut first"; hint = "Fully close CapCut (check the tray icon). This updates by itself."; }
  else if (!state.consent) { label = "Allow transcription to continue"; hint = "Tick the box above to send the audio to ElevenLabs."; }
  else { ok = true; if (i.has_captions) { label = "Regenerate captions"; hint = "Replaces the earlier auto-captions track."; } }
  b.textContent = label;
  b.disabled = !ok;
  $("hint").textContent = hint;
}

// restore: two clicks, because it also discards later edits made in CapCut
let restoreArmed = false;
$("restore").addEventListener("click", async () => {
  const btn = $("restore");
  if (!restoreArmed) {
    restoreArmed = true;
    btn.textContent = "Click again to confirm. This also undoes edits made in CapCut since.";
    btn.classList.add("confirm");
    setTimeout(() => { restoreArmed = false; btn.textContent = "Restore the backup from before the captions"; btn.classList.remove("confirm"); }, 6000);
    return;
  }
  restoreArmed = false;
  btn.textContent = "Restore the backup from before the captions";
  btn.classList.remove("confirm");
  try {
    await invoke("restore_backup", { folder: state.selected });
    note("restore-msg", "Restored. Open the project in CapCut.", "ok");
    refreshInfo();
  } catch (e) { note("restore-msg", msg(e), "bad"); }
});

// ------------------------------------------------------------------ run
const STEPS = ["transcribe", "captions", "write"];
function resetProgress() { show("steps", false); show("result", false); document.querySelectorAll("#steps li").forEach((li) => (li.className = "")); }
function setStep(name, status, message) {
  const idx = STEPS.indexOf(name);
  document.querySelectorAll("#steps li").forEach((li, n) => {
    if (n < idx) li.className = "done"; else if (n === idx) li.className = status;
    if (n === idx && message) li.querySelector(".lbl").textContent = message;
  });
}
function renderLines(lines) {
  const ul = $("lines");
  ul.replaceChildren();
  for (const l of lines) {
    const li = el("li");
    const txt = el("span", "txt");
    let last = 0;
    for (const m of l.text.matchAll(/[A-Za-z0-9]+(?:\s[A-Za-z0-9]+)*/g)) {
      if (m.index > last) txt.append(document.createTextNode(l.text.slice(last, m.index)));
      txt.append(el("span", "en", m[0]));
      last = m.index + m[0].length;
    }
    if (last < l.text.length) txt.append(document.createTextNode(l.text.slice(last)));
    li.append(el("time", "", fmtTime(l.start)), txt);
    ul.append(li);
  }
  $("result-title").textContent = `Captions (${lines.length})`;
  show("result", true);
}
function finish(ok, text) {
  state.running = false;
  document.querySelectorAll("#steps li.active").forEach((li) => (li.className = ok ? "done" : "fail"));
  if (ok) document.querySelectorAll("#steps li").forEach((li) => (li.className = "done"));
  note("result-note", text, ok ? "ok" : "bad");
  show("result", true);
  refreshInfo();
}
const FRIENDLY = [
  [/^consent_required/, "Tick the box to allow sending the audio to ElevenLabs, then try again."],
  [/No ElevenLabs key/, "No ElevenLabs key is saved. Add it in Settings, then Account."],
  [/CapCut is running/, "CapCut is open. Close it completely and try again."],
];
const friendly = (m) => (FRIENDLY.find(([re]) => re.test(m)) || [null, m])[1];

listen("caption-event", (e) => {
  const ev = e.payload;
  if (ev.event === "stage") setStep(ev.name, "active", ev.message);
  else if (ev.event === "usage") showRunUsage(ev);
  else if (ev.event === "captions") renderLines(ev.lines);
  else if (ev.event === "done") finish(true, `Done. ${ev.segments} captions added.${ev.skipped ? ` ${ev.skipped} clip(s) had no sound and were skipped.` : ""} Open CapCut, then open this project.`);
  else if (ev.event === "error") finish(false, friendly(ev.message));
});
$("go").addEventListener("click", async () => {
  if (state.running) return;
  if (!state.keySet) { openSettings("account"); return; }
  if (!state.selected) return;
  state.running = true;
  resetProgress();
  show("result-note", false);
  show("run-usage", false);
  $("lines").replaceChildren();
  show("steps", true);
  setStep("transcribe", "active");
  updateButton();
  try { await invoke("start_caption", { folder: state.selected, allowCloud: state.consent, clips: [...state.sel] }); }
  catch (e) { finish(false, msg(e)); }
});

// ------------------------------------------------------------------ settings drawer
const toHex = (c) => "#" + c.map((v) => Math.round(v * 255).toString(16).padStart(2, "0")).join("");
const fromHex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255);
let current = { english: { bold: true } };

function stylePreview() {
  paintPreview(["pv-si", "pv-en", "pv-si2"], { siFont: $("si-font").value, siSize: +$("si-size").value, enFont: $("en-font").value, enSize: +$("en-size").value, bold: $("en-bold").checked, color: $("color").value });
  $("si-size-out").textContent = $("si-size").value;
  $("en-size-out").textContent = $("en-size").value;
  const y = +$("pos-y").value;
  $("y-out").textContent = y < -0.3 ? "Low" : y > 0.3 ? "High" : "Middle";
  note("font-warn", isCollection($("si-font").value) || isCollection($("en-font").value) ? COLLECTION_WARN : "", "warn");
}
async function loadStyleTab() {
  try {
    const [fonts, s] = await Promise.all([invoke("list_fonts"), invoke("get_settings")]);
    state.fonts = fonts;
    current = s;
    fillFonts($("si-font"), fonts, s.sinhala.font);
    fillFonts($("en-font"), fonts, s.english.font);
    $("si-size").value = s.sinhala.size;
    $("en-size").value = s.english.size;
    $("en-bold").checked = s.english.bold;
    $("color").value = toHex(s.color);
    $("pos-y").value = s.position_y;
    stylePreview();
  } catch (e) { note("settings-msg", msg(e), "bad"); }
}
function selectTab(name) {
  document.querySelectorAll(".tab").forEach((t) => t.setAttribute("aria-selected", String(t.dataset.tab === name)));
  ["style", "words", "account", "usage", "about"].forEach((t) => show(`tab-${t}`, t === name));
  if (name === "style") loadStyleTab();
  if (name === "words") loadGlossary();
  if (name === "account") refreshAccount();
  if (name === "usage") loadUsage();
  if (name === "about") loadAbout();
}
function openSettings(tab) {
  show("scrim", true);
  show("settings", true);
  selectTab(tab || "style");
  $("settings").focus();
}
function closeSettings() { show("settings", false); show("scrim", false); $("open-settings").focus(); }
$("open-settings").addEventListener("click", () => openSettings("style"));
$("close-settings").addEventListener("click", closeSettings);
$("scrim").addEventListener("click", closeSettings);
document.addEventListener("keydown", (e) => { if (e.key === "Escape" && !$("settings").hidden) closeSettings(); });
document.querySelectorAll(".tab").forEach((t) => t.addEventListener("click", () => selectTab(t.dataset.tab)));
["si-font", "en-font", "si-size", "en-size", "en-bold", "color", "pos-y"].forEach((id) => $(id).addEventListener("input", stylePreview));
$("save-settings").addEventListener("click", async () => {
  const settings = {
    sinhala: { font: $("si-font").value, size: +$("si-size").value, bold: false },
    english: { font: $("en-font").value, size: +$("en-size").value, bold: $("en-bold").checked },
    color: fromHex($("color").value), position_y: +$("pos-y").value,
  };
  try { await invoke("save_settings", { settings }); note("settings-msg", "Saved. New captions will use this style.", "ok"); }
  catch (e) { note("settings-msg", msg(e), "bad"); }
});

// glossary editor
const gl = { english: [], spelling: [] };
function glRow(kind, i) {
  const [k, v] = gl[kind][i];
  const row = el("div", "gl-row");
  const a = el("input"), b = el("input"), x = el("button", "x", "×");
  a.value = k; b.value = v; a.maxLength = 80; b.maxLength = 80;
  a.placeholder = kind === "english" ? "වීඩියෝ" : "තවම"; b.placeholder = kind === "english" ? "Video" : "තාම";
  a.setAttribute("aria-label", "From"); b.setAttribute("aria-label", "To"); x.type = "button"; x.setAttribute("aria-label", "Remove");
  a.addEventListener("input", () => (gl[kind][i][0] = a.value));
  b.addEventListener("input", () => (gl[kind][i][1] = b.value));
  x.addEventListener("click", () => { gl[kind].splice(i, 1); glRender(); });
  row.append(a, b, x);
  return row;
}
function glRender() {
  for (const kind of ["english", "spelling"]) {
    const box = $(`gl-${kind}`);
    box.replaceChildren(...gl[kind].map((_, i) => glRow(kind, i)));
  }
}
async function loadGlossary() {
  try {
    const g = await invoke("get_glossary");
    gl.english = Object.entries(g.english || {});
    gl.spelling = Object.entries(g.spelling || {});
    glRender();
    note("gl-msg", "", "");
  } catch (e) { note("gl-msg", msg(e), "bad"); }
}
$("gl-add-english").addEventListener("click", () => { gl.english.push(["", ""]); glRender(); });
$("gl-add-spelling").addEventListener("click", () => { gl.spelling.push(["", ""]); glRender(); });
$("gl-save").addEventListener("click", async () => {
  const toObj = (rows) => Object.fromEntries(rows.filter(([k, v]) => k.trim() && v.trim()).map(([k, v]) => [k.trim(), v.trim()]));
  try {
    await invoke("save_glossary", { glossary: { english: toObj(gl.english), spelling: toObj(gl.spelling) } });
    note("gl-msg", "Saved. New captions will use these words.", "ok");
    loadGlossary();
  } catch (e) { note("gl-msg", msg(e), "bad"); }
});

// account (API key)
async function refreshAccount() {
  try {
    state.keySet = await invoke("key_status");
    note("acct-status", state.keySet ? "A key is saved on this PC." : "No key saved yet.", state.keySet ? "ok" : "warn");
    $("acct-remove").disabled = !state.keySet;
  } catch (e) { note("acct-status", msg(e), "bad"); }
  updateButton();
}
$("acct-save").addEventListener("click", () => saveKey("acct-key", "acct-msg", refreshAccount));
let removeArmed = false;
$("acct-remove").addEventListener("click", async () => {
  const btn = $("acct-remove");
  if (!removeArmed) {
    removeArmed = true; btn.textContent = "Click again to remove";
    setTimeout(() => { removeArmed = false; btn.textContent = "Remove key"; }, 5000);
    return;
  }
  removeArmed = false; btn.textContent = "Remove key";
  try { await invoke("delete_api_key"); note("acct-msg", "Key removed from this PC.", "ok"); } catch (e) { note("acct-msg", msg(e), "bad"); }
  refreshAccount();
});


// ------------------------------------------------------------------ usage
const fmtSecs = (s) => (s < 60 ? `${s.toFixed(1)} s` : `${Math.floor(s / 60)} min ${Math.round(s % 60)} s`);
const plural = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`;

function showRunUsage(u) {
  const parts = [`Sent ${fmtSecs(u.billed_seconds)} of audio in ${plural(u.billed_requests, "request")}`];
  if (u.cached_requests) parts.push(`${u.cached_requests} answered from the cache`);
  $("run-usage").textContent = parts.join(", ") + ".";
  show("run-usage", true);
}

function stat(value, label) {
  const d = el("div", "stat");
  d.append(el("b", "", value), el("span", "", label));
  return d;
}
function usageTable(headers, rows) {
  const t = el("table", "usage-table");
  const head = el("tr");
  headers.forEach((h) => head.append(el("th", "", h)));
  t.append(head);
  rows.forEach((r) => {
    const tr = el("tr");
    r.forEach((cell) => tr.append(el("td", "", String(cell))));
    t.append(tr);
  });
  return t;
}
async function loadUsage() {
  try {
    const u = await invoke("get_usage");
    state.price = u.price_per_minute;
    $("usage-price").value = u.price_per_minute == null ? "" : String(u.price_per_minute);
    const totals = $("usage-totals");
    totals.replaceChildren(stat(plural(u.runs, "run"), "caption runs"), stat(fmtSecs(u.billed_seconds), "audio sent"), stat(String(u.billed_requests), "paid requests"));
    if (u.price_per_minute != null) {
      const est = (u.billed_seconds / 60) * u.price_per_minute;
      totals.append(stat(est.toFixed(2), "rough cost estimate"), stat(String(u.cached_requests), "answers from cache"));
    }
    const box = $("usage-projects");
    box.replaceChildren();
    for (const p of u.projects) {
      const d = el("details", "usage-project");
      const sum = el("summary");
      sum.append(el("span", "", p.project), el("span", "sub", `${fmtSecs(p.billed_seconds)} sent, ${plural(p.runs, "run")}`));
      d.append(sum);
      d.append(el("p", "muted small-text", `Latest run: ${new Date(p.last.when * 1000).toLocaleString()}`));
      d.append(usageTable(["Request", "Audio", "Words", "Source"], p.last.requests.map((r, i) => [i + 1, fmtSecs(r.audio_s), r.words, r.cached ? "cache (free)" : "ElevenLabs"])));
      d.append(usageTable(["Clip", "At", "Audio", "Words", "Request"], p.last.clips.map((c) => [c.name, fmtDur(c.start), fmtSecs(c.audio_s), c.words, c.request])));
      box.append(d);
    }
    if (u.projects.length === 0) box.append(el("p", "muted", "Nothing yet. Usage appears here after your first caption run."));
    note("usage-msg", "", "");
  } catch (e) { note("usage-msg", msg(e), "bad"); }
}
$("usage-save-price").addEventListener("click", async () => {
  const raw = $("usage-price").value.trim().replace(",", ".");
  const price = raw === "" ? null : Number(raw);
  if (price !== null && (!Number.isFinite(price) || price < 0)) { note("usage-msg", "Enter a number, for example 0.40.", "bad"); return; }
  try { await invoke("set_price", { price }); note("usage-msg", price === null ? "Price removed." : "Saved.", "ok"); loadUsage(); }
  catch (e) { note("usage-msg", msg(e), "bad"); }
});
let clearArmed = false;
$("usage-clear").addEventListener("click", async () => {
  const btn = $("usage-clear");
  if (!clearArmed) {
    clearArmed = true; btn.textContent = "Click again to clear";
    setTimeout(() => { clearArmed = false; btn.textContent = "Clear usage log"; }, 5000);
    return;
  }
  clearArmed = false; btn.textContent = "Clear usage log";
  try { await invoke("clear_usage"); loadUsage(); } catch (e) { note("usage-msg", msg(e), "bad"); }
});

// ------------------------------------------------------------------ about
async function loadAbout() {
  try { const i = await invoke("app_info"); $("about-version").textContent = `version ${i.version}`; } catch { /* the page still reads fine */ }
  loadUpdateSettings();
}
$("about-coffee").addEventListener("click", async () => {
  try { await invoke("open_link", { which: "coffee" }); } catch { /* nothing useful to show */ }
});

// ------------------------------------------------------------------ updates
let offered = null;

function showOffer(r) {
  offered = r;
  $("update-title").textContent = `Version ${r.version} is available (you have ${r.current})`;
  $("update-notes").textContent = r.notes || "";
  show("update-notes", !!r.notes);
  note("update-msg", "", "");
  show("update-bar", false);
  $("update-now").disabled = false;
  $("update-later").disabled = false;
  show("update-banner", true);
}

async function lookForUpdate(quiet) {
  try {
    const r = await invoke("check_update");
    if (r.available) {
      showOffer(r);
      if (!quiet) note("update-status", `Version ${r.version} is available. Close Settings to update.`, "ok");
    } else if (!quiet) {
      note("update-status", `You are up to date (version ${r.current}).`, "ok");
    }
  } catch (e) {
    if (!quiet) note("update-status", msg(e), "warn");
  }
}

async function startupUpdateCheck() {
  try {
    const s = await invoke("get_update_settings");
    if (s.auto_check) lookForUpdate(true);
  } catch { /* an update check is never worth an error on startup */ }
}

async function loadUpdateSettings() {
  try {
    const s = await invoke("get_update_settings");
    $("auto-update").checked = s.auto_check;
  } catch { /* leave the box as it is */ }
}
$("auto-update").addEventListener("change", async (e) => {
  try { await invoke("set_auto_update_check", { enabled: e.target.checked }); }
  catch (err) { e.target.checked = !e.target.checked; note("update-status", msg(err), "bad"); }
});
$("check-update").addEventListener("click", () => {
  note("update-status", "Checking...", "");
  lookForUpdate(false);
});
$("update-later").addEventListener("click", () => show("update-banner", false));
$("update-now").addEventListener("click", async () => {
  if (!offered) return;
  $("update-now").disabled = true;
  $("update-later").disabled = true;
  note("update-msg", "Starting the download...", "");
  show("update-bar", true);
  try {
    await invoke("install_update");
    // On Windows the app closes and restarts itself, so this line is normally never reached.
    note("update-msg", "Installed. Restart the app to use the new version.", "ok");
  } catch (e) {
    note("update-msg", msg(e), "bad");
    show("update-bar", false);
    $("update-now").disabled = false;
    $("update-later").disabled = false;
  }
});
listen("update-event", (e) => {
  const ev = e.payload;
  if (ev.event === "progress") {
    const p = ev.percent;
    $("update-bar").firstElementChild.style.width = p == null ? "40%" : `${p}%`;
    note("update-msg", p == null ? "Downloading..." : `Downloading ${p}%`, "");
  } else if (ev.event === "installing") {
    $("update-bar").firstElementChild.style.width = "100%";
    note("update-msg", "Installing. The app will restart by itself.", "");
  }
});

// ------------------------------------------------------------------ start
boot();
setInterval(() => { if (state.selected && !state.running && !document.hidden && !$("view-main").hidden) refreshInfo(); }, 4000);
