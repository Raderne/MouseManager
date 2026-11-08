import { Meter, LEVELS, snapLevel, confirmLevel } from "./dpi.js";
import { cardModels, cardState, batteryText } from "./devices.js";

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
  // A value saved by the old free-form meter (e.g. 752) isn't a level: say so instead of quietly scaling from it.
  $("hw-dpi").textContent = !measured ? "pick a level" : LEVELS.includes(measured) ? `${measured} DPI` : `${Math.round(measured)} DPI (old reading: pick a level)`;
  for (const b of document.querySelectorAll(".stage.level")) {
    b.setAttribute("aria-pressed", String(Number(b.dataset.level) === measured));
  }
  for (const b of document.querySelectorAll(".stage:not(.level)")) {
    b.disabled = measured === null;
    b.setAttribute("aria-pressed", String((Number(b.dataset.dpi) || null) === target));
  }
  $("stage-hint").hidden = measured !== null;
  $("recent").textContent = meter.recent.length ? meter.recent.map(Math.round).join(" · ") : "—";
}

const saveDpi = () =>
  invoke("set_dpi", { measured, target }).catch((e) => ($("measure-status").textContent = String(e)));
const slideCm = () => Math.max(1, Number($("cm").value) || 10);

for (const b of document.querySelectorAll(".stage:not(.level)")) {
  b.addEventListener("click", () => {
    target = Number(b.dataset.dpi) || null;
    saveDpi();
    renderDpi();
  });
}

// The mouse's own DPI level: picked by the user (the PC can't see the button) or confirmed by a ruler check.
function setLevel(level) {
  measured = level;
  meter.last = level;
  saveDpi(); // re-scales the current pointer-speed stage
  renderDpi();
}
for (const b of document.querySelectorAll(".stage.level")) {
  b.addEventListener("click", () => {
    setLevel(Number(b.dataset.level));
    $("measure-status").textContent = `Mouse set to ${b.dataset.level} DPI. If you press its DPI button, pick the new level here.`;
  });
}

$("measure").addEventListener("click", () => {
  measuring = !measuring;
  meter.reset();
  meter.recent = [];
  $("measure").setAttribute("aria-pressed", measuring);
  $("measure").textContent = measuring ? "Stop checking" : "Check with a ruler";
  $("measure-status").textContent = measuring
    ? `Slide the N230 ${slideCm()} cm along a ruler and stop. Two slides that land on the same level set it.`
    : "";
});

// Monitor screen: buffered event list, redrawn at most every 100 ms.
const eventList = $("eventlist");
const monitorBuf = [];
const MAX_LINES = 1000;

const stamp = (d = new Date()) =>
  `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}:${String(d.getSeconds()).padStart(2, "0")}.${String(d.getMilliseconds()).padStart(3, "0")}`;

function fmtEvent(ev) {
  switch (ev.type) {
    case "button": return `${ev.target ? "N230 " : "other"} ${{ usb: "usb", bluetooth: "bt " }[ev.bus] ?? "   "} ${ev.button}-${ev.down ? "down" : "up"}  ${ev.device || ""}`;
    case "btInfo": return `bluetooth info ${ev.address}: ${ev.manufacturer ?? "—"}, firmware ${ev.firmware ?? "—"}`;
    case "settings": return `N230  settings report ${ev.hex}`;
    case "device": return `device ${ev.bus ? ev.bus + " " : ""}${ev.connected ? "connected" : "disconnected"}${ev.permission ? "" : " (no permission)"}`;
    case "link": return `link  ${ev.bus} ${ev.state}${ev.address ? " " + ev.address : ""}`;
    case "active": return `N230  now using ${ev.bus === "bluetooth" ? "Bluetooth" : "the USB receiver"}`;
    case "btAdapter": return `bluetooth adapter ${ev.state}`;
    case "btScan": return `bluetooth search ${ev.active ? "started" : "ended"}`;
    case "btFound": return `bluetooth found ${ev.name || "(no name)"} ${ev.address} ${ev.rssi} dBm${ev.n230 ? " [N230]" : ""}`;
    case "error": return `error  ${ev.message}`;
    case "battery": return `battery ${ev.percent ?? "—"}`;
    default: return JSON.stringify(ev);
  }
}

function addEventLine(ev) {
  if (ev.type === "move") return; // movement feeds the DPI meter, not the list
  if (ev.type === "settings") $("settings-report").textContent = ev.hex;
  monitorBuf.push(`${stamp()}  ${fmtEvent(ev)}`);
  if (monitorBuf.length > MAX_LINES) monitorBuf.splice(0, monitorBuf.length - MAX_LINES);
}

setInterval(() => {
  if (!monitorBuf.length) return;
  const lines = monitorBuf.splice(0, monitorBuf.length);
  const frag = document.createDocumentFragment();
  for (const line of lines) {
    const div = document.createElement("div");
    div.className = "line";
    div.textContent = line;
    frag.appendChild(div);
  }
  eventList.appendChild(frag);
  while (eventList.children.length > MAX_LINES) eventList.firstElementChild.remove();
  eventList.scrollTop = eventList.scrollHeight;
}, 100);

$("all-devices").addEventListener("change", (e) => invoke("set_all_devices", { on: e.target.checked }));
$("monitor-clear").addEventListener("click", () => eventList.replaceChildren());
$("monitor-copy").addEventListener("click", () =>
  navigator.clipboard.writeText([...eventList.children].map((d) => d.textContent).join("\n")).catch(() => console.error("copy failed"))
);

// Header battery: a real % from Rust, or "—" with an empty ring while unknown (asleep, on the receiver, Bluetooth off).
const BATTERY_C = 2 * Math.PI * 18; // the ring's r = 18
function renderBattery(percent) {
  const known = typeof percent === "number";
  $("battery").textContent = known ? `${percent}%` : "—";
  const ring = $("battery-ring");
  ring.toggleAttribute("hidden", !known); // an SVG <circle> has no .hidden property: set the attribute
  if (known) ring.setAttribute("stroke-dasharray", `${(BATTERY_C * percent) / 100} ${BATTERY_C}`);
}

// Battery, receiver/Bluetooth presence and the active link are often reported before this page
// listens: replay the latest values through the same handlers the events use.
invoke("get_status")
  .then((s) => {
    renderBattery(s.battery);
    onBtEvent({ type: "battery", percent: s.battery });
    if (s.usb !== null) onBtEvent({ type: "device", bus: "usb", connected: s.usb });
    if (s.active) onBtEvent({ type: "active", bus: s.active });
  })
  .catch(() => renderBattery(null));

listen("n230", ({ payload: ev }) => {
  if (ev.type === "battery") renderBattery(ev.percent);
  onBtEvent(ev); // device cards and the Add flow (ignores types it doesn't use)
  if (ev.type === "btFound" && !ev.mouse && !$("all-devices").checked) return; // keep the monitor about mice
  addEventLine(ev);
  if (measuring && ev.type === "move" && ev.target) meter.move(ev.dx, ev.dy, performance.now());
});

setInterval(() => {
  if (!measuring) return;
  const r = meter.tick(performance.now(), slideCm());
  if (!r) return;
  const status = $("measure-status");
  const level = r.ignored ? null : confirmLevel(meter.recent);
  if (r.ignored) {
    status.textContent = "Too short, ignored. Slide the full distance.";
  } else if (level) {
    status.textContent =
      level === measured
        ? `Checked: the mouse is on ${level} DPI (slides ${meter.recent.slice(0, 2).map(Math.round).join(", ")}).`
        : `The mouse is on ${level} DPI${measured ? ` (was set to ${Math.round(measured)})` : ""}.`;
    setLevel(level);
    meter.recent = []; // the next check starts fresh
  } else if (snapLevel(r.dpi) === null) {
    status.textContent = `Slide: ${Math.round(r.dpi)} DPI, not near 1000, 1200 or 1600. Check the distance and slide again.`;
  } else {
    status.textContent = `Slide: ${Math.round(r.dpi)} DPI → ${snapLevel(r.dpi)}. Slide again to confirm.`;
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
  setupSettings(config);
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

// Settings: general toggles and the device list.
const validId = (s) => /^[0-9A-Fa-f]{4}$/.test(s);
let devices = []; // last successfully saved list
// bus / address / name of each row, kept so editing VID:PID doesn't drop them. Rust validates and owns their meaning.
const rowExtra = new WeakMap();

const sayDevices = (text, error = false) => {
  const el = $("devices-status");
  el.textContent = text;
  el.classList.toggle("error", error);
};

function deviceRow(d) {
  const row = document.createElement("div");
  row.className = "device-row";
  const vid = document.createElement("input");
  vid.className = "vid";
  vid.maxLength = 4;
  vid.value = d.vid;
  vid.spellcheck = false;
  vid.setAttribute("aria-label", "VID");
  vid.placeholder = "VID";
  const sep = document.createElement("span");
  sep.className = "sep";
  sep.textContent = ":";
  const pid = document.createElement("input");
  pid.className = "pid";
  pid.maxLength = 4;
  pid.value = d.pid;
  pid.spellcheck = false;
  pid.setAttribute("aria-label", "PID");
  pid.placeholder = "PID";
  const del = document.createElement("button");
  del.className = "btn ghost";
  del.type = "button";
  del.textContent = "✕";
  del.setAttribute("aria-label", "Remove device");
  del.addEventListener("click", () => {
    row.remove();
    commitDevices(readDevices());
  });
  for (const input of [vid, pid]) input.addEventListener("change", () => commitDevices(readDevices()));
  const { vid: _v, pid: _p, ...extra } = d;
  rowExtra.set(row, extra);
  row.append(vid, sep, pid);
  if (d.name || d.bus) {
    const tag = document.createElement("span");
    tag.className = "device-tag";
    tag.textContent = d.name || (d.bus === "bluetooth" ? "Bluetooth" : "USB");
    row.append(tag);
  }
  row.append(del);
  return row;
}

function readDevices() {
  return [...$("devices").querySelectorAll(".device-row")].map((row) => ({
    ...rowExtra.get(row),
    vid: row.querySelector(".vid").value.trim().toUpperCase(),
    pid: row.querySelector(".pid").value.trim().toUpperCase(),
  }));
}

function commitDevices(list) {
  if (!list.every((d) => validId(d.vid) && validId(d.pid))) {
    sayDevices("Each VID and PID must be exactly 4 hex digits.", true);
    return;
  }
  invoke("set_devices", { devices: list }).then(
    () => {
      devices = list;
      sayDevices("Saved");
    },
    (e) => {
      sayDevices(String(e), true);
      renderDevices(devices); // revert to the last saved list
    }
  );
}

function renderDevices(list) {
  devices = list;
  const box = $("devices");
  box.replaceChildren();
  for (const d of list) box.appendChild(deviceRow(d));
  renderCards();
}

function setupSettings(config) {
  renderDevices(config.devices);

  const auto = $("autostart");
  invoke("get_autostart").then((on) => (auto.checked = on)).catch((e) => console.error("get_autostart:", e));
  auto.addEventListener("change", () =>
    invoke("set_autostart", { enabled: auto.checked }).then(
      () => {},
      (e) => {
        auto.checked = !auto.checked;
        console.error("set_autostart:", e);
      }
    )
  );

  $("start-minimized").checked = config.startMinimized;
  $("start-minimized").addEventListener("change", (e) =>
    invoke("set_start_minimized", { enabled: e.target.checked }).catch(() => (e.target.checked = !e.target.checked))
  );

  $("low-battery").checked = config.lowBatteryWarning;
  $("low-battery").addEventListener("change", (e) =>
    invoke("set_low_battery_warning", { enabled: e.target.checked }).catch(() => (e.target.checked = !e.target.checked))
  );

  $("device-add").addEventListener("click", () => $("devices").appendChild(deviceRow({ vid: "", pid: "" })));
  setupBluetooth();
}

// Devices and Bluetooth (B3–B7). Rust decides what each device is and what state it's in; JS only shows it.
const btFound = new Map(); // address → last btFound event
const linkStates = new Map(); // address → "connected" | "sleeping" | "disconnected" (Rust `link`)
const btInfoByAddr = new Map(); // address → { manufacturer, firmware } (Rust `btInfo`)
let pairedMice = []; // bt_paired: mice Windows has paired
let btScanning = false;
let btPairing = false;
let btJustPaired = false; // the next Bluetooth "connected" moves the flow on
let activeBus = null; // "usb" | "bluetooth" (Rust `active`)
let usbPresent = null; // receiver plugged in? null = not reported yet
let btBattery = null; // % over Bluetooth, null = unknown
let flowStep = 0; // 0 = flow closed

const ADAPTER_TEXT = {
  on: "Bluetooth is on.",
  off: "Bluetooth is off. Turn it on in the Windows settings to add or use a Bluetooth mouse.",
  missing: "No Bluetooth adapter found. If it was just disabled or unplugged, it's picked up again as soon as it's back.",
  denied: "Windows doesn't let this app use Bluetooth. Allow it in Settings → Privacy & security → Radios.",
};

function make(tag, className, text) {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

function sayBt(text, error = false) {
  $("bt-status").textContent = text;
  $("bt-status").classList.toggle("error", error);
}

function sayScan(text, error = false) {
  $("bt-scan-status").textContent = text;
  $("bt-scan-status").classList.toggle("error", error);
}

function showAdapter(state) {
  $("bt-adapter").textContent = ADAPTER_TEXT[state] ?? "—";
  $("bt-settings").hidden = state !== "off" && state !== "denied";
  $("bt-settings").textContent = state === "denied" ? "Open privacy settings" : "Open Bluetooth settings";
  $("bt-settings").dataset.page = state === "denied" ? "radios" : "bluetooth";
  $("bt-add").disabled = state !== "on" && flowStep === 0;
}

// A button that needs a second click within 4 s (forgetting or removing a mouse can't be undone in one click).
function confirmButton(label, armedLabel, onConfirm) {
  const b = make("button", "btn ghost", label);
  b.type = "button";
  let armed = null;
  b.addEventListener("click", () => {
    if (!armed) {
      b.textContent = armedLabel;
      armed = setTimeout(() => {
        armed = null;
        b.textContent = label;
      }, 4000);
      return;
    }
    clearTimeout(armed);
    armed = null;
    b.disabled = true;
    onConfirm(b);
  });
  return b;
}

// --- Device cards (rules in devices.js, checked by npm run check) ----------------------------

function renderCards() {
  const list = cardModels(devices, pairedMice);
  const box = $("device-cards");
  if (!list.length) {
    box.replaceChildren(make("li", "muted", "No devices yet. Add the N230 over Bluetooth, or plug in its receiver."));
    return;
  }
  box.replaceChildren(
    ...list.map((m) => {
      const li = make("li", "device-card");
      const name =
        m.device?.name || m.paired?.name || (m.kind === "usb" ? `USB device ${m.device.vid}:${m.device.pid}` : "Bluetooth mouse");
      const state = cardState(m, linkStates, activeBus, usbPresent);

      const head = make("div", "card-head");
      const icon = make("span", "conn-icon", m.kind === "usb" ? "USB" : "BT");
      icon.setAttribute("aria-hidden", "true");
      const title = make("span", "card-name", name);
      head.append(icon, title);
      if (m.kind === "bt-windows") head.append(make("span", "card-note", "Paired in Windows"));

      const facts = make("dl", "card-facts");
      const fact = (label, value) => facts.append(make("dt", "", label), make("dd", "", value));
      const stateDd = make("span", "bt-state" + (state.on ? " on" : ""), state.text);
      facts.append(make("dt", "", "Status"));
      const dd = make("dd");
      dd.append(stateDd);
      facts.append(dd);
      fact("Battery", batteryText(m, state, btBattery));
      if (m.kind !== "usb") {
        const info = btInfoByAddr.get(m.address);
        fact("Firmware", info?.firmware ?? "—");
        if (info?.manufacturer) fact("Made by", info.manufacturer);
        fact("Address", m.address);
      }

      const actions = make("div", "device-actions");
      if (m.kind !== "usb") {
        const forget = confirmButton("Forget", "Click again to forget", (b) => {
          b.textContent = "Forgetting…";
          invoke("bt_forget", { address: m.address })
            .then((list) => {
              renderDevices(list);
              sayBt(`Forgot ${name}. It's no longer paired with this PC.`);
              refreshPaired();
            })
            .catch((e) => {
              sayBt(String(e), true);
              refreshPaired();
            });
        });
        forget.setAttribute("aria-label", `Forget ${name} in Windows`);
        actions.append(forget);
      }
      if (m.device) {
        const remove = confirmButton("Remove from app", "Click again to remove", () => {
          const next = devices.filter((d) => d !== m.device);
          commitDevices(next);
          sayBt(`${name} is no longer remapped.${m.kind === "usb" ? "" : " It stays paired in Windows."}`);
        });
        remove.setAttribute("aria-label", `Stop remapping ${name}`);
        actions.append(remove);
      }

      li.append(head, facts, actions);
      return li;
    })
  );
}

function refreshPaired() {
  invoke("bt_paired")
    .then((list) => {
      pairedMice = list;
      renderCards();
    })
    .catch((e) => console.error("bt_paired:", e));
}

// --- Header link marker --------------------------------------------------------------------

function renderLinkChip() {
  const chip = $("link-chip");
  chip.hidden = !activeBus;
  chip.textContent = activeBus === "bluetooth" ? "BT" : "USB";
  chip.setAttribute("aria-label", activeBus === "bluetooth" ? "Connected over Bluetooth" : "Connected through the USB receiver");
  chip.title = chip.getAttribute("aria-label");
}

// --- Add Bluetooth mouse flow --------------------------------------------------------------

function gotoStep(n) {
  flowStep = n;
  $("bt-flow").hidden = n === 0;
  $("bt-add").hidden = n !== 0;
  for (const s of document.querySelectorAll("#bt-flow .flow-step")) s.hidden = Number(s.dataset.step) !== n;
  for (const li of document.querySelectorAll("#bt-flow .flow-steps li")) {
    const k = Number(li.dataset.for);
    li.classList.toggle("done", k < n);
    if (k === n) li.setAttribute("aria-current", "step");
    else li.removeAttribute("aria-current");
  }
  const first = document.querySelector(`#bt-flow .flow-step[data-step="${n}"] button`);
  if (first) first.focus();
}

function openFlow() {
  btFound.clear();
  renderBtFound();
  sayBt("");
  $("try-x1").classList.remove("on");
  $("try-x2").classList.remove("on");
  gotoStep(1);
}

function closeFlow() {
  if (btScanning) invoke("bt_scan", { on: false }).catch(() => {});
  gotoStep(0);
  $("bt-add").focus();
}

function startScan() {
  btFound.clear();
  renderBtFound();
  invoke("bt_scan", { on: true }).catch((e) => {
    sayScan(String(e), true);
    invoke("bt_adapter").then(showAdapter);
  });
}

function setScanning(on) {
  btScanning = on;
  $("bt-rescan").textContent = on ? "Stop" : "Search again";
  if (on) {
    sayScan("Searching… the N230 shows up within about 10 seconds.");
  } else {
    const mice = [...btFound.values()].filter((d) => d.mouse);
    sayScan(
      mice.length ? `Search finished: ${mice.length} ${mice.length === 1 ? "mouse" : "mice"} found. Pick yours and click Pair.` : "No mouse found. Is the N230 blinking (pairing mode)?",
      mice.length === 0
    );
  }
}

function renderBtFound() {
  const all = $("bt-all").checked;
  const list = [...btFound.values()].filter((d) => d.mouse || all).sort((a, b) => b.n230 - a.n230 || b.rssi - a.rssi);
  $("bt-found").replaceChildren(
    ...list.map((d) => {
      const li = make("li", "bt-item" + (d.n230 ? " n230" : ""));
      li.append(make("span", "bt-name", d.name || "Unnamed device"));
      if (d.n230) li.append(make("span", "bt-badge", "N230"));
      li.append(make("span", "bt-addr mono", d.address));
      const bars = make("span", "bt-bars");
      bars.setAttribute("role", "img");
      bars.setAttribute("aria-label", `Signal ${d.bars} of 4`);
      for (let i = 1; i <= 4; i++) bars.append(make("span", i <= d.bars ? "on" : ""));
      li.append(bars);
      if (d.mouse) {
        const pair = make("button", "btn", "Pair");
        pair.type = "button";
        pair.disabled = btPairing;
        pair.setAttribute("aria-label", `Pair ${d.name || d.address}`);
        pair.addEventListener("click", () => pairMouse(d));
        li.append(pair);
      }
      return li;
    })
  );
}

function pairMouse(d) {
  btPairing = true;
  gotoStep(3);
  $("bt-pair-text").textContent = `Pairing with ${d.name || d.address}…`;
  invoke("bt_pair", { address: d.address })
    .then(([list, message]) => {
      renderDevices(list);
      btFound.delete(d.address); // paired mice stop advertising
      btJustPaired = true;
      refreshPaired();
      if (flowStep) {
        $("bt-done-text").textContent = message;
        gotoStep(4);
      } else {
        sayBt(message); // the flow was hidden while pairing
      }
    })
    .catch((e) => {
      if (flowStep) {
        gotoStep(2);
        sayScan(String(e), true);
      } else {
        sayBt(String(e), true);
      }
    })
    .finally(() => {
      btPairing = false;
      renderBtFound();
    });
}

// --- Events --------------------------------------------------------------------------------

function onBtEvent(ev) {
  switch (ev.type) {
    case "btAdapter":
      showAdapter(ev.state);
      refreshPaired(); // the radio going off or on changes every link
      break;
    case "btScan":
      setScanning(ev.active);
      break;
    case "btFound":
      btFound.set(ev.address, ev);
      renderBtFound();
      break;
    case "btInfo":
      btInfoByAddr.set(ev.address, { manufacturer: ev.manufacturer, firmware: ev.firmware });
      renderCards();
      break;
    case "link":
      if (ev.address) linkStates.set(ev.address, ev.state);
      renderCards();
      break;
    case "battery":
      btBattery = ev.percent ?? null;
      renderCards();
      break;
    case "active":
      activeBus = ev.bus;
      $("bt-active").textContent = `The N230 is using ${ev.bus === "bluetooth" ? "Bluetooth" : "the USB receiver"}.`;
      renderLinkChip();
      renderCards();
      break;
    case "device":
      if (ev.bus === "usb") usbPresent = ev.connected;
      if (ev.bus === "bluetooth") {
        if (ev.connected && btJustPaired) {
          btJustPaired = false;
          if (flowStep === 4) $("bt-done-text").textContent = "Connected. Press a side button to check the remap.";
          else sayBt("Connected over Bluetooth.");
        }
        refreshPaired();
      }
      renderCards();
      break;
    case "button":
      if (flowStep === 4 && ev.target && ev.bus === "bluetooth" && (ev.button === "X1" || ev.button === "X2")) {
        $(ev.button === "X1" ? "try-x1" : "try-x2").classList.toggle("on", ev.down);
        if (ev.down) $("bt-done-text").textContent = `${ev.button} works over Bluetooth. Click Finish when you're done.`;
      }
      break;
  }
}

function setupBluetooth() {
  invoke("bt_adapter").then(showAdapter).catch((e) => console.error("bt_adapter:", e));
  refreshPaired();
  $("bt-add").addEventListener("click", openFlow);
  $("bt-search").addEventListener("click", () => {
    gotoStep(2);
    startScan();
  });
  $("bt-rescan").addEventListener("click", () => (btScanning ? invoke("bt_scan", { on: false }).catch(() => {}) : startScan()));
  for (const b of document.querySelectorAll("#bt-flow [data-cancel]")) b.addEventListener("click", closeFlow);
  $("bt-flow").addEventListener("keydown", (e) => {
    if (e.key === "Escape") closeFlow();
  });
  $("bt-settings").addEventListener("click", () =>
    invoke("bt_open_settings", { page: $("bt-settings").dataset.page || "bluetooth" }).catch((e) => console.error(e))
  );
  $("bt-all").addEventListener("change", renderBtFound);
  // The adapter can be switched on in Windows while this window is open.
  window.addEventListener("focus", () => invoke("bt_adapter").then(showAdapter).catch(() => {}));
}
