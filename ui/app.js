"use strict";

const tauri = window.__TAURI__;
const CATEGORIES = [
  { id: "all", label: "Übersicht", icon: '<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>' },
  { id: "windows", label: "Windows & Latenz", section: "System", icon: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>' },
  { id: "power", label: "Energie", icon: '<path d="M13 2 4 14h7l-1 8 9-12h-7z"/>' },
  { id: "gpu", label: "Grafik", icon: '<rect x="2" y="7" width="20" height="10" rx="2"/><circle cx="8" cy="12" r="2"/><circle cx="16" cy="12" r="2"/>' },
  { id: "storage", label: "Laufwerke", icon: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 14h18M7 17h.01"/>' },
  { id: "network", label: "Netzwerk", icon: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/>' },
  { id: "debloat", label: "Aufräumen", icon: '<path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/>' },
  { id: "fortnite", label: "Fortnite", section: "Spiele", icon: '<path d="M6 12h4M8 10v4M15 13h.01M18 11h.01"/><rect x="2" y="6" width="20" height="12" rx="4"/>' },
  { id: "gta", label: "GTA V", icon: '<path d="M5 17h14M6 17l1.5-6h9L18 17M8 11l1-3h6l1 3"/><circle cx="8" cy="17" r="2"/><circle cx="16" cy="17" r="2"/>' },
  { id: "games", label: "Weitere Spiele", icon: '<path d="m12 3 2.6 5.6 6 .7-4.4 4.2 1.1 6-5.3-3-5.3 3 1.1-6L3.4 9.3l6-.7z"/>' },
  { id: "hardware", label: "Hardware-Check", section: "Prüfen", icon: '<rect x="6" y="6" width="12" height="12" rx="2"/><path d="M9 2v4M15 2v4M9 18v4M15 18v4M2 9h4M2 15h4M18 9h4M18 15h4"/>' },
  { id: "bench", label: "Benchmark", page: true, icon: '<path d="M4 20V10M10 20V4M16 20v-7M22 20H2"/>' },
];
const TWEAK_CATEGORIES = CATEGORIES.filter((c) => !c.page && c.id !== "all");

const SAFE = ["kernel.global_timer_resolution", "bcd.remove_useplatformclock", "game.dvr_off", "input.mouse_accel_off", "power.fast_startup_off", "power.usb_suspend_off", "storage.trim", "storage.last_access_off", "debloat.telemetry", "debloat.telemetry_tasks", "debloat.consumer_features", "debloat.search_suggestions_off", "debloat.advertising_id_off", "debloat.startup_delay_off"];
const PRESETS = [
  { id: "safe", name: "Sicher", text: "Belegte Tweaks mit niedrigem Risiko: Timer, Eingabe, Telemetrie, Dateisystem. Passt für jedes System.", ids: SAFE },
  { id: "competitive", name: "Competitive", text: "Alles aus „Sicher“, dazu Timer-Dienst, Dialed-Energieplan, MSI-Mode, Prioritäten und die Fortnite-Basis. Sicherheitsfunktionen wie HVCI bleiben unangetastet.", ids: [...SAFE, "timer.service", "prio.win32_2a", "power.dialed_plan", "power.pcie_aspm_off", "power.throttling_off", "gpu.msi_mode", "net.eee_off", "debloat.background_apps", "fn.cpu_priority", "fn.io_priority", "fn.ini_competitive"] },
  { id: "notebook", name: "Notebook", text: "Alles aus „Sicher“, dazu weniger Hintergrundaktivität. Nichts, was Akkulaufzeit kostet.", ids: [...SAFE.filter((x) => x !== "power.usb_suspend_off"), "debloat.background_apps"] },
];
const EVIDENCE = {
  A: "Wirkung ist gemessen und reproduzierbar.",
  B: "Wirkung ist plausibel und teils gemessen, hängt aber vom System ab.",
  C: "Wirkung ist umstritten, es gibt nur Einzelberichte. Am besten mit dem Benchmark prüfen.",
};
// Einstufung: fasst Beleglage und Risiko zu einer verständlichen Gruppe zusammen
const TIERS = [
  { id: "recommended", label: "Empfohlen", hint: "Belegt oder plausibel, niedriges Risiko", cls: "ok" },
  { id: "optional", label: "Optional", hint: "Sinnvoll, aber mit Nebenwirkung, die du kennen solltest", cls: "out" },
  { id: "experimental", label: "Experimentell", hint: "Wirkung umstritten, selbst testen", cls: "warn" },
  { id: "risky", label: "Mit Vorsicht", hint: "Greift in Sicherheitsfunktionen ein", cls: "bad" },
  { id: "check", label: "Prüfungen", hint: "Wird nur erkannt, nicht geschaltet", cls: "out" },
  { id: "action", label: "Einmalige Aktionen", hint: "Kein Dauerzustand", cls: "out" },
];
function tierOf(t) {
  if (t.kind === "check") return "check";
  if (t.kind === "oneshot") return "action";
  if (t.risk === "high") return "risky";
  if (t.evidence === "C") return "experimental";
  if (t.risk === "medium" || t.irreversible) return "optional";
  return "recommended";
}
const tier = (id) => TIERS.find((x) => x.id === id);

const state = { bench: [], benchBusy: false, tweaks: [], hardware: null, category: "all", filter: "all", query: "", open: new Set(), busy: new Set(), loading: true };
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

// ---------- Backend ----------
async function invoke(cmd, args) {
  if (tauri) return tauri.core.invoke(cmd, args);
  return mockInvoke(cmd, args); // Vorschau im Browser ohne Tauri
}

function toast(text, isError) {
  const el = document.createElement("div");
  el.className = "toast" + (isError ? " err" : "");
  el.textContent = text;
  $("toasts").append(el);
  setTimeout(() => el.remove(), isError ? 9000 : 4000);
}

function confirmDialog(title, text, okLabel) {
  return new Promise((resolve) => {
    $("dlg-title").textContent = title;
    $("dlg-text").textContent = text;
    $("dlg-ok").textContent = okLabel || "Fortfahren";
    $("overlay").hidden = false;
    $("dlg-ok").focus();
    const done = (value) => {
      $("overlay").hidden = true;
      $("dlg-ok").onclick = $("dlg-cancel").onclick = null;
      document.removeEventListener("keydown", onKey);
      resolve(value);
    };
    const onKey = (e) => e.key === "Escape" && done(false);
    document.addEventListener("keydown", onKey);
    $("dlg-ok").onclick = () => done(true);
    $("dlg-cancel").onclick = () => done(false);
  });
}

async function scan() {
  state.loading = true;
  render();
  try {
    const res = await invoke("scan");
    state.tweaks = res.tweaks;
    state.hardware = res.hardware;
    state.scanMs = res.ms;
  } catch (e) {
    toast("Scan fehlgeschlagen: " + e, true);
  }
  state.loading = false;
  render();
}

function replaceTweak(view) {
  const i = state.tweaks.findIndex((t) => t.id === view.id);
  if (i >= 0) state.tweaks[i] = view;
}

async function toggle(id) {
  const t = state.tweaks.find((x) => x.id === id);
  if (!t || state.busy.has(id)) return;
  const turnOn = t.status !== "active";
  if (turnOn && (t.risk === "high" || t.irreversible)) {
    const text = (t.irreversible ? "Das lässt sich nicht automatisch rückgängig machen. " : "") + (t.note || t.description);
    if (!(await confirmDialog(t.name, text, "Aktivieren"))) return;
  }
  state.busy.add(id);
  render();
  try {
    const view = await invoke(turnOn ? "apply" : "revert", { id });
    replaceTweak(view);
    // Tweaks, die denselben Wert teilen (z. B. Win32PrioritySeparation), neu einlesen
    if (state.tweaks.some((x) => x.id !== id && shares(x, view))) await rescanQuiet();
    if (t.reboot) $("reboot").hidden = false;
    if (view.status !== (turnOn ? "active" : "inactive") && view.status !== "partial") {
      toast(`${t.name}: ${view.detail || "Zustand unverändert"}`, true);
    }
  } catch (e) {
    toast(`${t.name}: ${e}`, true);
  }
  state.busy.delete(id);
  render();
}

function shares(a, b) {
  const labels = new Set(b.actions.map((x) => x.label));
  return a.actions.some((x) => labels.has(x.label));
}

async function rescanQuiet() {
  try {
    const res = await invoke("scan");
    state.tweaks = res.tweaks;
    state.hardware = res.hardware;
  } catch (_) {}
}

async function runOneshot(id) {
  const t = state.tweaks.find((x) => x.id === id);
  if (!t || state.busy.has(id)) return;
  state.busy.add(id);
  render();
  try {
    const out = await invoke("run_oneshot", { id });
    toast(out || `${t.name}: erledigt`);
  } catch (e) {
    toast(`${t.name}: ${e}`, true);
  }
  state.busy.delete(id);
  render();
}

// ---------- Presets ----------
const presetTargets = (preset) => preset.ids.map((id) => state.tweaks.find((t) => t.id === id)).filter((t) => t && t.kind === "toggle" && (t.status === "inactive" || t.status === "partial"));

function openPresets() {
  $("preset-list").innerHTML = PRESETS.map((p) => {
    const n = presetTargets(p).length;
    return `<div class="preset"><div><h3>${esc(p.name)}</h3><p>${esc(p.text)}</p></div><button type="button" class="btn primary" data-preset="${p.id}" ${n ? "" : "disabled"}>${n ? `${n} aktivieren` : "Alles aktiv"}</button></div>`;
  }).join("");
  $("presets").hidden = false;
}

async function applyPreset(id) {
  const preset = PRESETS.find((p) => p.id === id);
  const targets = presetTargets(preset);
  $("presets").hidden = true;
  let failed = 0;
  for (const t of targets) {
    state.busy.add(t.id);
    render();
    try {
      replaceTweak(await invoke("apply", { id: t.id }));
      if (t.reboot) $("reboot").hidden = false;
    } catch (e) {
      failed++;
      toast(`${t.name}: ${e}`, true);
    }
    state.busy.delete(t.id);
  }
  await rescanQuiet();
  render();
  toast(`Preset „${preset.name}“: ${targets.length - failed} von ${targets.length} Tweaks aktiviert`);
}

// ---------- Benchmark ----------
async function loadBench() {
  try {
    state.bench = (await invoke("bench_history")) || [];
  } catch (_) {}
}

async function runBench() {
  const process = $("bench-process").value.trim();
  const seconds = parseInt($("bench-seconds").value, 10) || 60;
  state.benchBusy = true;
  renderBench();
  try {
    const run = await invoke("benchmark", { process, seconds });
    state.bench.push(run);
    toast(`Messung fertig: ${run.avg_fps} FPS im Schnitt, ${run.low1_fps} FPS 1%-Low`);
  } catch (e) {
    toast("Benchmark: " + e, true);
  }
  state.benchBusy = false;
  renderBench();
}

function renderBench() {
  const runs = [...state.bench].reverse();
  const diff = (a, b) => {
    if (b == null) return "";
    const d = Math.round((a - b) * 10) / 10;
    return d === 0 ? "" : ` <span class="${d > 0 ? "up" : "down"}">${d > 0 ? "+" : ""}${d}</span>`;
  };
  const rows = runs.map((r, i) => {
    const prev = runs[i + 1] && runs[i + 1].process === r.process ? runs[i + 1] : null;
    const names = r.active.map((id) => (state.tweaks.find((t) => t.id === id) || { name: id }).name).join(", ");
    return `<tr><td>${new Date(r.at * 1000).toLocaleString("de-DE", { dateStyle: "short", timeStyle: "short" })}</td><td>${esc(r.process)}</td><td class="num">${r.avg_fps}${diff(r.avg_fps, prev && prev.avg_fps)}</td><td class="num">${r.low1_fps}${diff(r.low1_fps, prev && prev.low1_fps)}</td><td class="num">${r.low01_fps}${diff(r.low01_fps, prev && prev.low01_fps)}</td><td title="${esc(names)}">${r.active.length} aktiv</td></tr>`;
  }).join("");
  $("list").innerHTML = `
    <article class="card benchform">
      <div class="row"><div>
        <div class="head"><h3>Spiel messen</h3></div>
        <p>Misst Frametimes des laufenden Spiels mit Intel PresentMon. Spiel starten, immer dieselbe Szene spielen und die Messung vor und nach einem Tweak vergleichen. Zu jeder Messung merkt sich Dialed, welche Tweaks aktiv waren.</p>
        <div class="fields">
          <label>Prozess<input id="bench-process" value="FortniteClient-Win64-Shipping.exe" spellcheck="false"></label>
          <label>Dauer<select id="bench-seconds"><option value="30">30 s</option><option value="60" selected>60 s</option><option value="120">120 s</option></select></label>
          <button type="button" class="btn primary" id="bench-run" ${state.benchBusy ? "disabled" : ""}>${state.benchBusy ? "Messung läuft …" : "Messung starten"}</button>
        </div>
        <p class="hint">PresentMon einmalig installieren: <code>winget install --id Intel.PresentMon.Console -e</code></p>
      </div></div>
    </article>
    ${rows ? `<article class="card"><div class="details shown"><table class="bench"><thead><tr><th>Wann</th><th>Prozess</th><th class="num">Ø FPS</th><th class="num">1%-Low</th><th class="num">0,1%-Low</th><th>Tweaks</th></tr></thead><tbody>${rows}</tbody></table></div></article>` : '<div class="empty">Noch keine Messungen.</div>'}`;
}

// ---------- Darstellung ----------
const isOn = (t) => t.status === "active" || t.status === "partial";
const countable = (t) => t.kind !== "oneshot" && t.status !== "unsupported";

function visible() {
  const q = state.query.trim().toLowerCase();
  return state.tweaks.filter((t) => {
    if (state.category !== "all" && t.category !== state.category) return false;
    if (state.filter === "on" && !isOn(t)) return false;
    if (state.filter === "off" && (isOn(t) || t.kind === "oneshot" || t.status === "unsupported")) return false;
    if (q && !(t.name + " " + t.description + " " + t.id).toLowerCase().includes(q)) return false;
    return true;
  });
}

function renderNav() {
  $("nav").innerHTML = CATEGORIES.map((c) => {
    const items = state.tweaks.filter((t) => (c.id === "all" || t.category === c.id) && countable(t));
    const on = items.filter((t) => t.status === "active").length;
    const count = c.page || (state.loading && !state.tweaks.length) ? "" : `<span class="count">${on}/${items.length}</span>`;
    const section = c.section ? `<div class="navsec">${esc(c.section)}</div>` : "";
    return `${section}<button type="button" data-cat="${c.id}" ${state.category === c.id ? 'aria-current="page"' : ""}><svg viewBox="0 0 24 24" aria-hidden="true">${c.icon}</svg>${esc(c.label)}${count}</button>`;
  }).join("");
}

function renderHardware() {
  const hw = state.hardware;
  if (!hw || !hw.cpu) {
    $("hw").innerHTML = `<h2>System</h2><div class="muted">${state.loading ? "Wird erkannt …" : "Nicht erkannt"}</div>`;
    return;
  }
  const row = (k, v) => (v ? `<div title="${esc(v)}"><span class="k">${k}</span>${esc(v)}</div>` : "");
  const cpu = hw.cpu.replace(/\((R|TM)\)/gi, "").replace(/\s+\d+-Core Processor/i, "").replace(/\s+CPU.*$/i, "").trim();
  $("hw").innerHTML =
    `<h2>${hw.laptop ? "Notebook" : "Desktop"}</h2>` +
    row("CPU", cpu) +
    hw.gpus.map((g) => row("GPU", g)).join("") +
    row("RAM", `${hw.ram_gb} GB${hw.ram_speed ? ` @ ${hw.ram_speed} MHz` : ""}`) +
    row("OS", `${hw.os.replace("Microsoft ", "")} (${hw.build})`) +
    (hw.flags.includes("fortnite") ? row("Spiel", "Fortnite gefunden") : "") +
    (hw.flags.includes("gta_legacy") ? row("Spiel", "GTA V Legacy gefunden") : "") +
    (hw.flags.includes("gta_enhanced") ? row("Spiel", "GTA V Enhanced gefunden") : "");
}

function statusBadge(t) {
  if (t.status === "active") return t.kind === "check" ? '<span class="badge ok">Erfüllt</span>' : '<span class="badge active">Aktiv</span>';
  if (t.status === "partial") return '<span class="badge warn">Teilweise aktiv</span>';
  if (t.status === "unsupported") return '<span class="badge out">Nicht verfügbar</span>';
  if (t.status === "error") return '<span class="badge bad">Fehler</span>';
  if (t.kind === "check") return '<span class="badge warn">Offen</span>';
  return "";
}

function card(t, showTier) {
  const busy = state.busy.has(t.id);
  const tr = tier(tierOf(t));
  const badges = [
    statusBadge(t),
    t.drift ? '<span class="badge bad" title="Von Dialed gesetzt, aber inzwischen wieder geändert, etwa durch ein Windows- oder Treiber-Update">Abweichung</span>' : "",
    showTier && t.kind === "toggle" ? `<span class="badge ${tr.cls}" title="${esc(tr.hint)}">${esc(tr.label)}</span>` : "",
    t.reboot ? '<span class="badge out" title="Wirkt erst nach einem Neustart">Neustart</span>' : "",
  ].join("");

  let control = "";
  if (t.kind === "toggle") {
    const checked = t.status === "active" ? "true" : t.status === "partial" ? "mixed" : "false";
    const locked = t.status === "unsupported" || t.status === "error" || (t.irreversible && t.status === "active");
    control = `<button type="button" role="switch" class="switch${busy ? " busy" : ""}" aria-checked="${checked}" aria-label="${esc(t.name)}" data-toggle="${esc(t.id)}" ${locked ? "disabled" : ""}></button>`;
  } else if (t.kind === "oneshot") {
    control = `<button type="button" class="btn outline" data-run="${esc(t.id)}" ${busy || t.status === "unsupported" ? "disabled" : ""}>${busy ? "Läuft …" : "Ausführen"}</button>`;
  }

  const detail = t.detail && t.status !== "active" ? `<div class="state muted">${esc(t.detail)}</div>` : "";
  const rows = t.actions
    .map((a) => `<tr><td class="mono"><span class="dot ${a.state}"></span>${esc(a.label)}</td><td class="mono">${esc(a.target)}</td><td class="mono">${esc(a.current)}</td></tr>`)
    .join("");
  const table = rows ? `<table><thead><tr><th>Einstellung</th><th>Ziel</th><th>Aktuell</th></tr></thead><tbody>${rows}</tbody></table>` : "";
  const open = state.open.has(t.id);
  const cls = ["card", t.status === "active" && t.kind === "toggle" ? "on" : "", t.status === "unsupported" ? "na" : "", open ? "open" : ""].join(" ");

  return `<article class="${cls}" data-id="${esc(t.id)}">
    <div class="row">
      <div>
        <div class="head"><h3>${esc(t.name)}</h3>${badges}</div>
        <p>${esc(t.description)}</p>${detail}
      </div>
      <div class="ctl">${control}<button type="button" class="more" data-more="${esc(t.id)}" aria-label="Details" aria-expanded="${open}"><svg viewBox="0 0 24 24"><path d="m6 9 6 6 6-6"/></svg></button></div>
    </div>
    <div class="details">${t.note ? `<div class="note">${esc(t.note)}</div>` : ""}${t.kind === "toggle" ? `<p class="evidence">${esc(EVIDENCE[t.evidence] || "")}</p>` : ""}${table}</div>
  </article>`;
}

const byName = (a, b) => a.name.localeCompare(b.name, "de");

function grouped(items) {
  return TIERS.map((tr) => {
    const group = items.filter((t) => tierOf(t) === tr.id).sort(byName);
    return group.length ? `<div class="group">${esc(tr.label)}<span>${esc(tr.hint)}</span></div>` + group.map((t) => card(t, false)).join("") : "";
  }).join("");
}

function overview() {
  const tiles = TWEAK_CATEGORIES.map((c) => {
    const items = state.tweaks.filter((t) => t.category === c.id && countable(t));
    const on = items.filter((t) => t.status === "active").length;
    const pct = items.length ? Math.round((on / items.length) * 100) : 0;
    return `<button type="button" class="tile" data-cat="${c.id}"><svg viewBox="0 0 24 24" aria-hidden="true">${c.icon}</svg><strong>${esc(c.label)}</strong><span>${on} von ${items.length} aktiv</span><i><b style="width:${pct}%"></b></i></button>`;
  }).join("");
  const drift = state.tweaks.filter((t) => t.drift);
  const open = state.tweaks.filter((t) => tierOf(t) === "recommended" && (t.status === "inactive" || t.status === "partial")).sort(byName);
  const failed = state.tweaks.filter((t) => t.status === "error");
  const section = (title, hint, items) => (items.length ? `<div class="group">${esc(title)}<span>${esc(hint)}</span></div>` + items.map((t) => card(t, false)).join("") : "");
  return (
    `<div class="tiles">${tiles}</div>` +
    section("Abweichungen", "Von Dialed gesetzt, aber inzwischen wieder geändert", drift) +
    section("Fehler", "Diese Einträge ließen sich nicht auslesen", failed) +
    (open.length ? section("Empfohlen und noch nicht aktiv", "Belegt oder plausibel, niedriges Risiko", open) : '<div class="empty">Alle empfohlenen Tweaks sind aktiv.</div>')
  );
}

function renderList() {
  const list = $("list");
  const page = state.category === "bench";
  const home = state.category === "all" && !state.query.trim() && state.filter === "all";
  $("toolbar").hidden = page;
  if (page) return renderBench();
  if (state.loading && !state.tweaks.length) {
    list.innerHTML = '<div class="skeleton"></div>'.repeat(6);
    return;
  }
  if (home) {
    list.innerHTML = overview();
    return;
  }
  const items = visible();
  if (!items.length) {
    list.innerHTML = '<div class="empty">Keine Tweaks für diese Auswahl.</div>';
    return;
  }
  // Suche und Filter über alle Kategorien: flache Liste mit Einstufung als Badge
  list.innerHTML = state.category === "all" ? items.sort(byName).map((t) => card(t, true)).join("") : grouped(items);
}

function render() {
  const cat = CATEGORIES.find((c) => c.id === state.category);
  $("title").textContent = cat.label;
  $("tb-page").textContent = cat.label;
  renderNav();
  renderHardware();
  renderList();
  const all = state.tweaks.filter(countable);
  const on = all.filter((t) => t.status === "active").length;
  $("summary").textContent = state.tweaks.length ? `${on} von ${all.length} aktiv${state.scanMs != null ? ` · Scan ${state.scanMs} ms` : ""}` : "";
  $("btn-scan").disabled = state.loading;
  $("btn-scan").textContent = state.loading ? "Scannt …" : "Neu scannen";
}

// ---------- Ereignisse ----------
$("nav").addEventListener("click", (e) => {
  const btn = e.target.closest("[data-cat]");
  if (!btn) return;
  state.category = btn.dataset.cat;
  $("main").scrollTop = 0;
  render();
});
$("filters").addEventListener("click", (e) => {
  const btn = e.target.closest("[data-filter]");
  if (!btn) return;
  state.filter = btn.dataset.filter;
  for (const b of $("filters").children) b.setAttribute("aria-selected", String(b === btn));
  renderList();
});
$("search").addEventListener("input", (e) => {
  state.query = e.target.value;
  renderList();
});
$("list").addEventListener("click", (e) => {
  const sw = e.target.closest("[data-toggle]");
  if (sw) return void toggle(sw.dataset.toggle);
  const run = e.target.closest("[data-run]");
  if (run) return void runOneshot(run.dataset.run);
  const more = e.target.closest("[data-more]");
  if (more) {
    const id = more.dataset.more;
    state.open.has(id) ? state.open.delete(id) : state.open.add(id);
    more.closest(".card").classList.toggle("open");
    more.setAttribute("aria-expanded", String(state.open.has(id)));
  }
});
$("list").addEventListener("click", (e) => {
  if (e.target.id === "bench-run") return void runBench();
  const tile = e.target.closest(".tile");
  if (tile) {
    state.category = tile.dataset.cat;
    $("main").scrollTop = 0;
    render();
  }
});
$("btn-presets").addEventListener("click", openPresets);
$("presets").addEventListener("click", (e) => {
  const btn = e.target.closest("[data-preset]");
  if (btn) return void applyPreset(btn.dataset.preset);
  if (e.target.id === "presets" || e.target.id === "presets-close") $("presets").hidden = true;
});
$("btn-scan").addEventListener("click", scan);
$("btn-restore").addEventListener("click", async () => {
  const btn = $("btn-restore");
  btn.disabled = true;
  btn.textContent = "Wird erstellt …";
  try {
    await invoke("restore_point");
    toast("Wiederherstellungspunkt „Dialed“ erstellt");
  } catch (e) {
    toast("Wiederherstellungspunkt: " + e, true);
  }
  btn.disabled = false;
  btn.textContent = "Wiederherstellungspunkt";
});
document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
    e.preventDefault();
    $("search").focus();
  }
});

// Fenstersteuerung der eigenen Titelleiste
if (tauri && tauri.window) {
  const win = tauri.window.getCurrentWindow();
  $("win-min").addEventListener("click", () => win.minimize());
  $("win-max").addEventListener("click", () => win.toggleMaximize());
  $("win-close").addEventListener("click", () => win.close());
}

// Updates: beim Start einmal prüfen, Installation per Klick
async function checkUpdate() {
  if (!tauri) return;
  try {
    const update = await invoke("check_update");
    if (update) {
      $("update-version").textContent = "Version " + update.version;
      $("update").hidden = false;
    }
  } catch (_) {
    // offline oder noch kein Release veröffentlicht
  }
}
$("btn-update").addEventListener("click", async () => {
  const btn = $("btn-update");
  btn.disabled = true;
  btn.textContent = "Wird installiert …";
  try {
    await invoke("install_update");
  } catch (e) {
    toast("Update: " + e, true);
    btn.disabled = false;
    btn.textContent = "Installieren";
  }
});
checkUpdate();

// Hell/Dunkel folgt dem System
const scheme = window.matchMedia("(prefers-color-scheme: dark)");
const applyScheme = () => document.documentElement.classList.toggle("dark", scheme.matches);
scheme.addEventListener("change", applyScheme);
applyScheme();

// ---------- Browser-Vorschau ohne Tauri ----------
let mockData;
async function mockInvoke(cmd, args) {
  await new Promise((r) => setTimeout(r, 350));
  if (!mockData) mockData = await (await fetch("mock.json")).json();
  if (cmd === "scan") return mockData;
  if (cmd === "bench_history") return [{ at: 1794000000, process: "FortniteClient-Win64-Shipping.exe", seconds: 60, frames: 14100, avg_fps: 235.1, low1_fps: 148.3, low01_fps: 96.2, active: ["game.dvr_off"] }, { at: 1794003600, process: "FortniteClient-Win64-Shipping.exe", seconds: 60, frames: 14400, avg_fps: 240.4, low1_fps: 162.9, low01_fps: 110.5, active: ["game.dvr_off", "prio.win32_2a", "kernel.global_timer_resolution"] }];
  if (cmd === "apply" || cmd === "revert") {
    const t = mockData.tweaks.find((x) => x.id === args.id);
    t.status = cmd === "apply" ? "active" : "inactive";
    t.managed = cmd === "apply";
    t.drift = false;
    t.actions.forEach((a) => (a.state = cmd === "apply" ? "applied" : "open"));
    return t;
  }
  return "";
}

loadBench();
scan();
