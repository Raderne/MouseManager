// Run: npm run check
import assert from "node:assert/strict";
import { cardModels, cardState, batteryText } from "../src/devices.js";

const receiver = { vid: "3554", pid: "FC01", bus: "usb", name: "N230 receiver" };
const anyBt = { vid: "25A7", pid: "FAA2", bus: "bluetooth", name: "N230 Bluetooth" }; // default entry, no address
const bt1 = { vid: "25A7", pid: "FAA2", bus: "bluetooth", address: "CC:CD:72:DF:59:E5", name: "Lenovo Services N230 BT1" };
const bt1Paired = { address: "CC:CD:72:DF:59:E5", name: "Lenovo Services N230 BT1", n230: true, connected: true };
const bt2Windows = { address: "DB:13:6A:63:29:D1", name: "Lenovo Services N230 BT2", n230: true, connected: false };

// Which cards exist
{
  const m = cardModels([receiver, anyBt, bt1], [bt1Paired, bt2Windows]);
  assert.deepEqual(
    m.map((c) => c.kind),
    ["usb", "bt", "bt-windows"],
    "receiver, BT1 from the app, BT2 paired in Windows; the address-less default entry has no card"
  );
  assert.equal(m[1].paired, bt1Paired, "the app's entry is joined with what Windows says");
  assert.equal(cardModels([{ ...bt1, address: "cc:cd:72:df:59:e5" }], [bt1Paired]).length, 1, "address case doesn't create a duplicate card");
  assert.deepEqual(cardModels([], []), [], "nothing configured, nothing paired");
}

const none = new Map();

// Receiver card
{
  const [usb] = cardModels([receiver], []);
  assert.deepEqual(cardState(usb, none, "usb", true), { text: "In use", on: true });
  assert.equal(cardState(usb, none, "bluetooth", true).text, "Plugged in");
  assert.equal(cardState(usb, none, null, false).text, "Unplugged");
  assert.equal(cardState(usb, none, null, null).text, "Receiver", "not reported yet: no guess");
  assert.equal(batteryText(usb, cardState(usb, none, null, true), 83), "— (not reported over the receiver)", "never the BT battery on the receiver card");
}

// Bluetooth card
{
  const [bt] = cardModels([bt1], [bt1Paired]);
  const links = new Map([["CC:CD:72:DF:59:E5", "sleeping"]]);
  assert.equal(cardState(bt, links, "bluetooth", true).text, "Asleep or off: move the mouse to wake it");
  assert.equal(cardState(bt, links, "usb", true).text, "Not in use: the N230 is on the USB receiver", "on the receiver, not 'asleep'");
  const up = cardState(bt, new Map([["CC:CD:72:DF:59:E5", "connected"]]), "usb", true);
  assert.deepEqual(up, { text: "Connected", on: true, link: "connected" }, "a connected link wins over the active bus");
  assert.equal(cardState(bt, none, null, null).link, "connected", "no link event yet: fall back to Windows' connected flag");
  assert.equal(batteryText(bt, up, 83), "83%");
  assert.equal(batteryText(bt, cardState(bt, links, "bluetooth", true), 83), "—", "asleep: no stale number");
  assert.equal(batteryText(bt, up, null), "—", "connected but not read yet");
}

// Paired in Windows only: the battery event belongs to watched (app-paired) mice, so never shown here
{
  const [, w] = cardModels([receiver], [bt2Windows]);
  const s = cardState(w, new Map([[bt2Windows.address, "connected"]]), "bluetooth", true);
  assert.equal(s.text, "Connected");
  assert.equal(batteryText(w, s, 83), "—");
}

console.log("devices check OK");
