"use strict";

const START = String.raw`\psi (y; t+\varepsilon)=\int_{-\infty}^{\infty}\psi(x; t)\int_{x(t)=x}^{x(t+\varepsilon)=y}e^{i\int_{t}^{t+\varepsilon}\left(\frac{1}{2}\dot{x}^{2}-V(x)\right)dt}Dx(t)dx`;
const FONT_SIZE = 64;
const SVG_NS = "http://www.w3.org/2000/svg";

const $ = (id) => document.getElementById(id);
const stage = $("stage"), keys = $("keys"), texField = $("tex"), status = $("status");

const KEY_CODES = {
  ArrowLeft: 1, ArrowRight: 2, ArrowUp: 3, ArrowDown: 4, Home: 5, End: 6,
  Backspace: 7, Delete: 8, Escape: 12, Enter: 13,
};

let wasm, wasmBytes = 0, info = null, svg = null, hovered = -1, timing = null, benchTimer = 0;
const encoder = new TextEncoder(), decoder = new TextDecoder();

// --- toggles, remembered per browser ------------------------------------------------------

const toggles = {};
function loadToggles() {
  let saved = {};
  try { saved = JSON.parse(localStorage.getItem("eeltex-toggles") || "{}"); } catch {}
  for (const box of document.querySelectorAll("[data-toggle]")) {
    const name = box.dataset.toggle;
    if (name in saved) box.checked = !!saved[name];
    toggles[name] = box.checked;
    box.addEventListener("change", () => {
      toggles[name] = box.checked;
      try { localStorage.setItem("eeltex-toggles", JSON.stringify(toggles)); } catch {}
      applyPanels();
      drawOverlay();
      showStats();
    });
  }
  applyPanels();
}
function applyPanels() {
  $("stats-panel").classList.toggle("hidden", !toggles.stats);
  $("hover-panel").classList.toggle("hidden", !toggles.hover);
  $("panels").classList.toggle("hidden", !toggles.stats && !toggles.hover);
}

// --- the wasm boundary --------------------------------------------------------------------

function writeInput(text) {
  const bytes = encoder.encode(text);
  const ptr = wasm.alloc(bytes.length);
  new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
}
function readOutput(which) {
  const ptr = wasm.out_ptr(which), len = wasm.out_len(which);
  return decoder.decode(new Uint8Array(wasm.memory.buffer, ptr, len));
}

/** Run one call into the editor, then redraw if it changed anything. */
function call(fn) {
  const changed = fn();
  if (changed) refresh();
  return changed;
}

/** Time layout and export once typing pauses; one keystroke is too quick for the clock. */
function scheduleBench() {
  clearTimeout(benchTimer);
  benchTimer = setTimeout(() => {
    timing = { layout: wasm.bench_layout(25), svg: wasm.bench_svg(25) };
    showStats();
  }, 200);
}

function refresh() {
  info = JSON.parse(readOutput(1));
  stage.querySelector("svg")?.remove();
  $("loading")?.remove();
  stage.insertAdjacentHTML("beforeend", readOutput(0));
  svg = stage.querySelector("svg");
  svg.setAttribute("aria-hidden", "true");
  fitToStage();
  if (document.activeElement !== texField) {
    texField.value = info.tex;
    texField.classList.remove("bad");
    growTexField();
  }
  setStatus(info.error);
  hovered = -1;
  drawOverlay();
  showNode();
  showStats();
  scheduleBench();
}

function setStatus(error) {
  let text = "", bad = true;
  if (error) text = error;
  else if (info.pending != null) { text = `typing \\${info.pending}: Enter to insert, Esc to cancel`; bad = false; }
  else if (info.unknown.length) text = `Unknown command ${info.unknown.join(", ")}. Backspace after it to fix the name.`;
  status.textContent = text;
  status.classList.toggle("bad", bad && !!text);
}

// --- overlay, in scene coordinates ----------------------------------------------------------

function rect(parent, cls, x, y, w, h, extra) {
  const r = document.createElementNS(SVG_NS, "rect");
  r.setAttribute("class", cls);
  r.setAttribute("x", x); r.setAttribute("y", y);
  r.setAttribute("width", Math.max(w, 0)); r.setAttribute("height", Math.max(h, 0));
  if (extra) for (const [k, v] of Object.entries(extra)) r.setAttribute(k, v);
  parent.appendChild(r);
}
function line(parent, cls, x1, y1, x2, y2, extra) {
  const l = document.createElementNS(SVG_NS, "line");
  l.setAttribute("class", cls);
  l.setAttribute("x1", x1); l.setAttribute("y1", y1);
  l.setAttribute("x2", x2); l.setAttribute("y2", y2);
  if (extra) for (const [k, v] of Object.entries(extra)) l.setAttribute(k, v);
  parent.appendChild(l);
}
const depthColour = (depth) => `hsl(${(depth * 47 + 160) % 360} 70% 50%)`;

function drawOverlay() {
  if (!svg || !info) return;
  svg.querySelector(".ov")?.remove();
  const ov = document.createElementNS(SVG_NS, "g");
  ov.setAttribute("class", "ov");

  if (info.selection) {
    const [x, y, w, h] = info.selection;
    rect(ov, "selection", x, y, w, h);
  }
  if (toggles.metrics) {
    rect(ov, "metric", 0, -info.ascent, info.width, info.ascent + info.descent);
    line(ov, "baseline", 0, 0, info.width, 0);
  }
  info.nodes.forEach(([kind, , depth, x, y, w, h, baseline, placeholder], i) => {
    if (w <= 0 || h <= 0 || i === 0) return;
    const colour = depthColour(depth);
    if (toggles.boxes) {
      rect(ov, placeholder ? "node placeholder" : "node", x, y, w, h, { stroke: colour });
    }
    if (toggles.baselines) line(ov, "node", x, baseline, x + w, baseline, { stroke: colour });
  });
  if (toggles.hover && hovered > 0) {
    const [, , , x, y, w, h] = info.nodes[hovered];
    rect(ov, "hover", x, y, w, h);
  }
  if (toggles.caret && info.caret) {
    const [x, y, t, h] = info.caret;
    rect(ov, "caret", x - t / 2, y, t, h);
  }
  svg.appendChild(ov);
}

// Shrink a big formula to fit the stage, however small that makes it: it never overflows.
const MAX_HEIGHT = () => Math.max(160, window.innerHeight * 0.55);
let displayScale = 1;
function fitToStage() {
  if (!svg) return;
  const w = parseFloat(svg.getAttribute("width")), h = parseFloat(svg.getAttribute("height"));
  const style = getComputedStyle(stage);
  const room = stage.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
  displayScale = Math.max(0, Math.min(1, w > 0 ? room / w : 1, h > 0 ? MAX_HEIGHT() / h : 1));
  svg.style.width = `${w * displayScale}px`;
  svg.style.height = `${h * displayScale}px`;
}
window.addEventListener("resize", () => {
  fitToStage();
  showStats();
  growTexField();
});

// --- panels ------------------------------------------------------------------------------------

const fmt = (n, digits = 1) => Number(n).toFixed(digits);
function fillList(dl, rows) {
  dl.replaceChildren();
  for (const [k, v] of rows) {
    const dt = document.createElement("dt"), dd = document.createElement("dd");
    dt.textContent = k; dd.textContent = v;
    dl.append(dt, dd);
  }
}

function showStats() {
  if (!info || !toggles.stats) return;
  const us = (ms) => (timing ? `${fmt(ms * 1000, ms < 0.01 ? 1 : 0)} µs` : "…");
  const maxDepth = info.nodes.reduce((m, n) => Math.max(m, n[2]), 0);
  fillList($("stats"), [
    ["layout", us(timing?.layout)],
    ["SVG export", us(timing?.svg)],
    ["SVG size", `${(info.svg_bytes / 1024).toFixed(1)} KiB`],
    ["glyphs · rules", `${info.glyphs} · ${info.rules}`],
    ["nodes · depth", `${info.nodes.length} · ${maxDepth}`],
    ["width", `${fmt(info.width)} px`],
    ["ascent · descent", `${fmt(info.ascent)} · ${fmt(info.descent)} px`],
    ["font size · shown at", `${info.font_size} px · ${Math.round(displayScale * 100)}%`],
    ["missing glyphs", info.missing],
    ["undo · redo", `${info.undo ? "yes" : "no"} · ${info.redo ? "yes" : "no"}`],
    ["wasm", `${(wasmBytes / 1024).toFixed(0)} KiB`],
  ]);
}

function showNode() {
  const dl = $("node");
  if (!info || hovered < 0) {
    fillList(dl, [["node", "—"]]);
    return;
  }
  const [kind, parent, depth, x, y, w, h, baseline, placeholder] = info.nodes[hovered];
  const glyphs = info.glyph_nodes.filter((n) => n === hovered).length;
  fillList(dl, [
    ["node", `#${hovered}${placeholder ? " (empty slot)" : ""}`],
    ["kind", kind],
    ["parent", parent < 0 ? "—" : `#${parent} ${info.nodes[parent][0]}`],
    ["depth", depth],
    ["box", `${fmt(x)}, ${fmt(y)}  ${fmt(w)} × ${fmt(h)}`],
    ["baseline", fmt(baseline)],
    ["own glyphs", glyphs],
  ]);
}

// --- input ---------------------------------------------------------------------------------------

function scenePoint(event) {
  const ctm = svg?.getScreenCTM();
  if (!ctm) return null;
  const p = new DOMPoint(event.clientX, event.clientY).matrixTransform(ctm.inverse());
  return [p.x, p.y];
}

function focusKeys() {
  keys.focus({ preventScroll: true });
}

keys.addEventListener("keydown", (e) => {
  const mod = e.ctrlKey || e.metaKey;
  if (mod) {
    const k = e.key.toLowerCase();
    if (k === "z" && !e.shiftKey) { e.preventDefault(); call(() => wasm.key(10, 0, 0)); return; }
    if (k === "y" || (k === "z" && e.shiftKey)) { e.preventDefault(); call(() => wasm.key(11, 0, 0)); return; }
    if (k === "a") { e.preventDefault(); call(() => wasm.key(9, 0, 0)); return; }
    if (k === "c" || k === "x") {
      if (wasm.copy()) {
        e.preventDefault();
        navigator.clipboard?.writeText(readOutput(2)).catch(() => {});
        if (k === "x") call(() => wasm.key(7, 0, 0));
      }
      return;
    }
    if (k === "v") return; // the paste event below
  }
  // Enter submits a raw `\\command` while one is typed, and adds a row otherwise.
  if (e.key === "Enter" && info?.pending != null) {
    e.preventDefault();
    call(() => wasm.key(14, 0, 0));
    return;
  }
  const code = KEY_CODES[e.key];
  if (code) {
    e.preventDefault();
    call(() => wasm.key(code, e.shiftKey ? 1 : 0, mod || e.altKey ? 1 : 0));
  } else if (e.key === "Tab") {
    e.preventDefault();
    call(() => wasm.key(e.shiftKey ? 1 : 2, 0, 0));
  }
});

// Typed text arrives through `input`, so IME and on-screen keyboards work too.
keys.addEventListener("input", () => {
  const text = keys.value;
  keys.value = "";
  for (const ch of text) call(() => wasm.insert(ch.codePointAt(0)));
});

keys.addEventListener("paste", (e) => {
  e.preventDefault();
  const text = e.clipboardData?.getData("text/plain");
  if (text) { writeInput(text); call(() => wasm.paste()); }
});

let dragging = false;
stage.addEventListener("pointerdown", (e) => {
  if (e.target === texField) return;
  e.preventDefault();
  focusKeys();
  const p = scenePoint(e);
  if (!p) return;
  call(() => wasm.click(p[0], p[1], e.shiftKey ? 1 : 0));
  dragging = true;
  stage.setPointerCapture(e.pointerId);
});
stage.addEventListener("pointerup", () => { dragging = false; });
stage.addEventListener("dblclick", (e) => {
  const p = scenePoint(e);
  if (p) call(() => wasm.select_at(p[0], p[1]));
});
stage.addEventListener("pointermove", (e) => {
  const p = scenePoint(e);
  if (!p) return;
  if (dragging) call(() => wasm.click(p[0], p[1], 1));
  const hit = wasm.hit(p[0], p[1]);
  if (hit !== hovered) {
    hovered = hit;
    drawOverlay();
    showNode();
  }
});
stage.addEventListener("pointerleave", () => {
  if (hovered !== -1) { hovered = -1; drawOverlay(); showNode(); }
});

/** Grow the source box to its contents, so it never scrolls. */
function growTexField() {
  texField.style.height = "auto";
  texField.style.height = `${texField.scrollHeight + texField.offsetHeight - texField.clientHeight}px`;
}

let texTimer = 0;
texField.addEventListener("input", () => {
  growTexField();
  clearTimeout(texTimer);
  texTimer = setTimeout(() => {
    writeInput(texField.value);
    const ok = wasm.set_tex();
    refresh();
    texField.classList.toggle("bad", !ok);
  }, 120);
});
texField.addEventListener("blur", () => {
  if (info && !texField.classList.contains("bad")) texField.value = info.tex;
  growTexField();
});

// --- start -------------------------------------------------------------------------------------

async function start() {
  loadToggles();
  const response = await fetch("eeltex_demo.wasm");
  const bytes = await response.arrayBuffer();
  wasmBytes = bytes.byteLength;
  const { instance } = await WebAssembly.instantiate(bytes, { env: { now: () => performance.now() } });
  wasm = instance.exports;
  writeInput(START);
  if (!wasm.init(FONT_SIZE)) throw new Error("the embedded font did not load");
  refresh();
  focusKeys();
}

start().catch((err) => {
  $("loading").textContent = `failed to load: ${err.message}`;
  status.classList.add("bad");
});
