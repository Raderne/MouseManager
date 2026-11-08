// Device cards (Settings → Devices): which cards exist and what each says. Pure functions, no DOM,
// checked by `npm run check` (tests/devices.mjs). The facts (bus, link state, n230, battery) all come
// from Rust; this only decides how to show them.

export const LINK_TEXT = {
  connected: "Connected",
  sleeping: "Asleep or off: move the mouse to wake it",
  disconnected: "Not connected",
};

/**
 * One card per configured receiver, per Bluetooth entry paired from the app (it has an address), and
 * per mouse Windows paired on its own. A Bluetooth entry without an address (the default "any N230
 * over Bluetooth" entry) gets no card: it has no state of its own.
 * @param {Array<{vid,pid,bus?,address?,name?}>} devices  config.devices
 * @param {Array<{address,name,n230,connected}>} paired   bt_paired
 */
export function cardModels(devices, paired) {
  const models = [];
  const known = new Set();
  for (const d of devices) {
    if (d.bus === "usb") {
      models.push({ kind: "usb", device: d });
    } else if (d.bus === "bluetooth" && d.address) {
      known.add(d.address.toUpperCase());
      models.push({ kind: "bt", device: d, address: d.address, paired: paired.find((m) => m.address.toUpperCase() === d.address.toUpperCase()) });
    }
  }
  for (const m of paired) {
    if (!known.has(m.address.toUpperCase())) models.push({ kind: "bt-windows", address: m.address, paired: m });
  }
  return models;
}

/**
 * What a card's Status line says.
 * @param model     from cardModels
 * @param linkStates Map address → "connected" | "sleeping" | "disconnected" (Rust `link`)
 * @param activeBus  "usb" | "bluetooth" | null (Rust `active`)
 * @param usbPresent true | false | null (Rust `device` for the receiver)
 * @returns {{text: string, on?: boolean, link?: string}}
 */
export function cardState(model, linkStates, activeBus, usbPresent) {
  if (model.kind === "usb") {
    if (activeBus === "usb") return { text: "In use", on: true };
    if (usbPresent === false) return { text: "Unplugged" };
    return { text: usbPresent ? "Plugged in" : "Receiver" };
  }
  const link = linkStates.get(model.address) ?? (model.paired?.connected ? "connected" : "disconnected");
  const isN230 = model.paired?.n230 || /N230/i.test(model.device?.name ?? "");
  if (link !== "connected" && activeBus === "usb" && isN230) {
    return { text: "Not in use: the N230 is on the USB receiver", link };
  }
  return { text: LINK_TEXT[link] ?? link, on: link === "connected", link };
}

/**
 * The Battery line. Only a Bluetooth mouse paired from the app, while connected, has a real value: the
 * battery event isn't tied to an address, and the receiver reports none. Never a stale number.
 */
export function batteryText(model, state, battery) {
  if (model.kind === "usb") return "— (not reported over the receiver)";
  return model.kind === "bt" && state.link === "connected" && typeof battery === "number" ? `${battery}%` : "—";
}
