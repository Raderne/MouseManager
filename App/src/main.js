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

// Monitor screen: buffered event list, redrawn at most every 100 ms.
const eventList = $("eventlist");
const monitorBuf = [];
const MAX_LINES = 1000;

const stamp = (d = new Date()) =>
  `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}:${String(d.getSeconds()).padStart(2, "0")}.${String(d.getMilliseconds()).padStart(3, "0")}`;

function fmtEvent(ev) {
  switch (ev.type) {
    case "button": return `${ev.target ? "N230 " : "other"} ${ev.button}-${ev.down ? "down" : "up"}  ${ev.device || ""}`;
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

listen("n230", ({ payload: ev }) => {
  if (ev.type.startsWith("bt") || ["device", "link", "active"].includes(ev.type)) {
    onBtEvent(ev);
    if (ev.type === "btFound" && !ev.mouse && !$("all-devices").checked) return; // keep the monitor about mice
  }
  addEventLine(ev);
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

// Bluetooth search (B3). Rust decides what a device is (mouse, n230, bars); JS only shows it.
const btFound = new Map(); // address → last btFound event
let btScanning = false;

const ADAPTER_TEXT = {
  on: "Bluetooth is on.",
  off: "Bluetooth is off. Turn it on in the Windows settings to search.",
  missing: "No Bluetooth LE adapter found on this PC.",
};

function showAdapter(state) {
  $("bt-adapter").textContent = ADAPTER_TEXT[state] ?? "—";
  $("bt-settings").hidden = state !== "off";
  $("bt-search").disabled = state !== "on" && !btScanning;
}

function renderBtFound() {
  const all = $("bt-all").checked;
  const list = [...btFound.values()]
    .filter((d) => d.mouse || all)
    .sort((a, b) => b.n230 - a.n230 || b.rssi - a.rssi);
  $("bt-found").replaceChildren(
    ...list.map((d) => {
      const li = document.createElement("li");
      li.className = "bt-item" + (d.n230 ? " n230" : "");
      const name = document.createElement("span");
      name.className = "bt-name";
      name.textContent = d.name || "Unnamed device";
      li.append(name);
      if (d.n230) {
        const badge = document.createElement("span");
        badge.className = "bt-badge";
        badge.textContent = "N230";
        li.append(badge);
      }
      const addr = document.createElement("span");
      addr.className = "bt-addr mono";
      addr.textContent = d.address;
      const bars = document.createElement("span");
      bars.className = "bt-bars";
      bars.setAttribute("role", "img");
      bars.setAttribute("aria-label", `Signal ${d.bars} of 4`);
      for (let i = 1; i <= 4; i++) {
        const b = document.createElement("span");
        if (i <= d.bars) b.className = "on";
        bars.append(b);
      }
      li.append(addr, bars);
      if (d.mouse) {
        const pair = document.createElement("button");
        pair.className = "btn";
        pair.type = "button";
        pair.textContent = "Pair";
        pair.disabled = btPairing;
        pair.setAttribute("aria-label", `Pair ${d.name || d.address}`);
        pair.addEventListener("click", () => pairMouse(d));
        li.append(pair);
      }
      return li;
    })
  );
  return list;
}

let btPairing = false;
let btJustPaired = false; // the next Bluetooth "connected" event finishes the flow

function sayBt(text, error = false) {
  $("bt-status").textContent = text;
  $("bt-status").classList.toggle("error", error);
}

function pairMouse(d) {
  btPairing = true;
  renderBtFound();
  sayBt(`Pairing with ${d.name || d.address}… keep the mouse close and in pairing mode.`);
  invoke("bt_pair", { address: d.address })
    .then(([list, message]) => {
      renderDevices(list);
      btFound.delete(d.address); // paired mice stop advertising
      btJustPaired = true;
      sayBt(message);
      refreshPaired();
    })
    .catch((e) => sayBt(String(e), true))
    .finally(() => {
      btPairing = false;
      renderBtFound();
    });
}

const linkStates = new Map(); // address → "connected" | "sleeping" | "disconnected", from Rust `link` events
let lastPaired = [];
let activeBus = null; // "usb" | "bluetooth", from Rust `active` events

const LINK_TEXT = {
  connected: "Connected",
  sleeping: "Asleep or off: move the mouse to wake it",
  disconnected: "Not connected",
};

function renderPaired(list) {
  lastPaired = list;
  $("bt-paired-empty").hidden = list.length > 0;
  $("bt-paired").replaceChildren(
    ...list.map((m) => {
      const li = document.createElement("li");
      li.className = "bt-item" + (m.n230 ? " n230" : "");
      const name = document.createElement("span");
      name.className = "bt-name";
      name.textContent = m.name || "Unnamed mouse";
      const state = document.createElement("span");
      const link = linkStates.get(m.address) ?? (m.connected ? "connected" : "disconnected");
      state.className = "bt-state" + (link === "connected" ? " on" : "");
      state.textContent =
        link !== "connected" && activeBus === "usb" && m.n230
          ? "Not in use: the N230 is on the USB receiver"
          : LINK_TEXT[link] ?? link;
      const addr = document.createElement("span");
      addr.className = "bt-addr mono";
      addr.textContent = m.address;
      const forget = document.createElement("button");
      forget.className = "btn ghost";
      forget.type = "button";
      forget.textContent = "Forget";
      forget.setAttribute("aria-label", `Forget ${m.name || m.address}`);
      let armed = null; // two clicks: forgetting means pairing again later
      forget.addEventListener("click", () => {
        if (!armed) {
          forget.textContent = "Click again to forget";
          armed = setTimeout(() => {
            armed = null;
            forget.textContent = "Forget";
          }, 4000);
          return;
        }
        clearTimeout(armed);
        forget.disabled = true;
        forget.textContent = "Forgetting…";
        invoke("bt_forget", { address: m.address })
          .then((devices) => {
            renderDevices(devices);
            sayBt(`Forgot ${m.name || m.address}. It's no longer paired with this PC.`);
            refreshPaired();
          })
          .catch((e) => {
            sayBt(String(e), true);
            refreshPaired();
          });
      });
      li.append(name);
      if (m.n230) {
        const badge = document.createElement("span");
        badge.className = "bt-badge";
        badge.textContent = "N230";
        li.append(badge);
      }
      li.append(state, addr, forget);
      return li;
    })
  );
}

function refreshPaired() {
  invoke("bt_paired").then(renderPaired).catch((e) => console.error("bt_paired:", e));
}

function setScanning(on) {
  btScanning = on;
  $("bt-search").textContent = on ? "Stop" : "Search for mouse";
  $("bt-search").disabled = false;
  if (on) {
    $("bt-status").classList.remove("error");
    $("bt-status").textContent = "Searching…";
  } else {
    const mice = [...btFound.values()].filter((d) => d.mouse);
    $("bt-status").classList.toggle("error", mice.length === 0);
    $("bt-status").textContent = mice.length
      ? `Search finished: ${mice.length} ${mice.length === 1 ? "mouse" : "mice"} found.`
      : "No mouse found. Is the N230 in pairing mode?";
  }
}

function onBtEvent(ev) {
  if (ev.type === "device" && ev.bus === "bluetooth") {
    if (ev.connected && btJustPaired) {
      btJustPaired = false;
      sayBt("Connected. Try a side button.");
    }
    refreshPaired();
  } else if (ev.type === "link" && ev.address) {
    linkStates.set(ev.address, ev.state);
    renderPaired(lastPaired);
  } else if (ev.type === "active") {
    activeBus = ev.bus;
    $("bt-active").textContent = `The N230 is using ${ev.bus === "bluetooth" ? "Bluetooth" : "the USB receiver"}.`;
    renderPaired(lastPaired);
  } else if (ev.type === "btAdapter") {
    showAdapter(ev.state);
    refreshPaired(); // the radio going off or on changes every link
  }
  else if (ev.type === "btScan") setScanning(ev.active);
  else if (ev.type === "btFound") {
    btFound.set(ev.address, ev);
    renderBtFound();
  }
}

function setupBluetooth() {
  invoke("bt_adapter").then(showAdapter).catch((e) => console.error("bt_adapter:", e));
  refreshPaired();
  $("bt-search").addEventListener("click", () => {
    if (!btScanning) {
      btFound.clear();
      renderBtFound();
    }
    invoke("bt_scan", { on: !btScanning }).catch((e) => {
      $("bt-status").classList.add("error");
      $("bt-status").textContent = String(e);
      invoke("bt_adapter").then(showAdapter);
    });
  });
  $("bt-settings").addEventListener("click", () => invoke("bt_open_settings").catch((e) => console.error(e)));
  $("bt-all").addEventListener("change", renderBtFound);
  // The adapter can be switched on in Windows while this window is open.
  window.addEventListener("focus", () => invoke("bt_adapter").then(showAdapter).catch(() => {}));
}
