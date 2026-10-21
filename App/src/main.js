import { Meter } from "./dpi.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (id) => document.getElementById(id);

// Pill tabs: show the clicked tab's section, hide the rest. No router.
const tabs = document.querySelectorAll('[role="tab"]');
for (const tab of tabs) {
  tab.addEventListener("click", () => {
    for (const t of tabs) {
      const on = t === tab;
      t.setAttribute("aria-selected", on);
      document.getElementById(t.getAttribute("aria-controls")).hidden = !on;
    }
  });
}

// Buttons screen. Mapping text is only sent to Rust, never parsed here.
const PRESETS = [
  ["Default", "Default (no change)"],
  ["Back", "Back"],
  ["Forward", "Forward"],
  ["Middle", "Middle click"],
  ["Disabled", "Disabled"],
  ["Alt+Left", "Alt+Left"],
  ["Alt+Right", "Alt+Right"],
  ["Ctrl+C", "Ctrl+C"],
  ["Ctrl+V", "Ctrl+V"],
  ["Ctrl+Z", "Ctrl+Z"],
  ["Ctrl+Shift+T", "Ctrl+Shift+T"],
  ["Win+Tab", "Win+Tab"],
  ["Win+D", "Win+D"],
  ["MediaPlayPause", "Media play / pause"],
  ["VolumeUp", "Volume up"],
  ["VolumeDown", "Volume down"],
];
const CUSTOM = "\u0000custom";
const isPreset = (text) => PRESETS.some(([v]) => v === text);

function selectButton(button) {
  document.getElementById("diagram").dataset.selected = button;
  for (const row of document.querySelectorAll(".map-row")) {
    const on = row.dataset.button === button;
    row.classList.toggle("selected", on);
    row.querySelector(".chip").setAttribute("aria-pressed", on);
  }
}

function setupRow(row, current) {
  const button = row.dataset.button;
  const select = row.querySelector("select");
  const custom = row.querySelector(".custom");
  const input = custom.querySelector("input");
  const status = row.querySelector(".status");
  let saved = current;
  let timer;

  for (const [value, label] of PRESETS) select.add(new Option(label, value));
  select.add(new Option("Custom…", CUSTOM));

  const show = (text) => {
    select.value = isPreset(text) ? text : CUSTOM;
    custom.hidden = isPreset(text);
    if (!isPreset(text)) input.value = text;
  };
  const say = (text, error = false) => {
    clearTimeout(timer);
    status.textContent = text;
    status.classList.toggle("error", error);
    if (!error) timer = setTimeout(() => (status.textContent = ""), 2000);
  };
  const save = async (text) => {
    try {
      await invoke("set_mapping", { button, mapping: text });
      saved = text;
      say("Saved");
    } catch (e) {
      say(String(e), true); // the old mapping is still active
      if (isPreset(saved) && select.value !== CUSTOM) show(saved);
    }
  };

  show(current);
  select.addEventListener("change", () => {
    if (select.value === CUSTOM) {
      custom.hidden = false;
      input.value = isPreset(saved) ? "" : saved;
      input.focus();
    } else {
      custom.hidden = true;
      save(select.value);
    }
  });
  custom.querySelector("button").addEventListener("click", () => save(input.value));
  input.addEventListener("keydown", (e) => e.key === "Enter" && save(input.value));
  row.querySelector(".chip").addEventListener("click", () => selectButton(button));
  row.addEventListener("focusin", () => selectButton(button));
}

// Performance screen. Rust turns measured + target into the movement scale.
const DIAL_MAX = 1600;
const DIAL_C = 2 * Math.PI * 72;
let measured = null; // confirmed hardware DPI
let target = null; // chosen stage, null = Native
let meter = new Meter();
let measuring = false;

function renderDpi() {
  const effective = measured && target ? target : measured;
  $("dial-value").textContent = effective ? Math.round(effective) : "—";
  const ring = $("dial-ring");
  ring.toggleAttribute("hidden", !effective); // SVG elements have no .hidden property
  ring.setAttribute("stroke-dasharray", `${(DIAL_C * Math.min(effective ?? 0, DIAL_MAX)) / DIAL_MAX} ${DIAL_C}`);
  $("hw-dpi").textContent = measured ? Math.round(measured) : "—";
  for (const b of document.querySelectorAll(".stage")) {
    b.disabled = measured === null;
    b.setAttribute("aria-pressed", String((Number(b.dataset.dpi) || null) === target));
  }
  $("stage-hint").hidden = measured !== null;
  $("recent").textContent = meter.recent.length ? meter.recent.map(Math.round).join(" · ") : "—";
}

const saveDpi = () =>
  invoke("set_dpi", { measured, target }).catch((e) => ($("measure-status").textContent = String(e)));
const slideCm = () => Math.max(1, Number($("cm").value) || 10);

for (const b of document.querySelectorAll(".stage")) {
  b.addEventListener("click", () => {
    target = Number(b.dataset.dpi) || null;
    saveDpi();
    renderDpi();
  });
}

$("measure").addEventListener("click", () => {
  measuring = !measuring;
  meter.reset();
  $("measure").setAttribute("aria-pressed", measuring);
  $("measure").textContent = measuring ? "Stop measuring" : "Measure DPI";
  $("measure-status").textContent = measuring
    ? `Slide the N230 ${slideCm()} cm in a straight line and stop. Do it twice.`
    : "";
});

listen("n230", ({ payload: ev }) => {
  if (measuring && ev.type === "move" && ev.target) meter.move(ev.dx, ev.dy, performance.now());
});

setInterval(() => {
  if (!measuring) return;
  const r = meter.tick(performance.now(), slideCm());
  if (!r) return;
  const status = $("measure-status");
  if (r.ignored) {
    status.textContent = "Too short, ignored. Slide the full distance.";
  } else if (!r.confirmed) {
    status.textContent = `Slide: ${Math.round(r.dpi)} DPI. Slide again to confirm.`;
  } else {
    status.textContent = r.changed
      ? `DPI changed: now ${Math.round(r.confirmed)} (was ${Math.round(measured)}).`
      : `Measured ${Math.round(r.confirmed)} DPI.`;
    measured = r.confirmed;
    saveDpi(); // a new hardware DPI re-scales the current stage
  }
  renderDpi();
}, 100);

invoke("get_config").then((config) => {
  for (const row of document.querySelectorAll(".map-row")) setupRow(row, config.buttons[row.dataset.button]);
  selectButton("X2");
  measured = config.measuredDpi;
  target = config.targetDpi;
  meter = new Meter(measured);
  renderDpi();
});

// Theme picker. Rust set data-theme before first paint; recolor at once, then save.
const root = document.documentElement;
for (const radio of document.querySelectorAll('input[name="theme"]')) {
  radio.checked = radio.value === root.dataset.theme;
  radio.addEventListener("change", () => {
    root.dataset.theme = radio.value;
    invoke("set_theme", { theme: radio.value }).catch((e) => console.error("set_theme:", e));
  });
}
