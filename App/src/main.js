const { invoke } = window.__TAURI__.core;

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

invoke("get_config").then((config) => {
  for (const row of document.querySelectorAll(".map-row")) setupRow(row, config.buttons[row.dataset.button]);
  selectButton("X2");
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
