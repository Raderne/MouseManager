//! Windows: WinRT `BluetoothLEAdvertisementWatcher` for searching, `BluetoothAdapter` + `Radio` for adapter state.
//! WinRT calls run on our own threads (implicit MTA), never on the UI thread or the input hook thread.

use super::{appearance, format_address, is_n230, parse_address, AdapterState, Advert, Links, Paired, PairedMouse, Scan, AD_APPEARANCE, SCAN_SECS};
use crate::{
    device::{Bus, DeviceId},
    event::{Event, LinkState},
};
use std::{
    sync::{mpsc::Sender, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use windows::{
    core::{IInspectable, Ref, RuntimeType, GUID, HSTRING},
    Devices::{
        Bluetooth::{
            Advertisement::{
                BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementWatcher,
                BluetoothLEAdvertisementWatcherStoppedEventArgs, BluetoothLEScanningMode,
            },
            BluetoothAdapter, BluetoothAddressType, BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice,
            GenericAttributeProfile::{
                GattCharacteristic, GattCharacteristicUuids, GattClientCharacteristicConfigurationDescriptorValue, GattCommunicationStatus,
                GattServiceUuids, GattValueChangedEventArgs,
            },
        },
        Enumeration::{
            DeviceInformation, DeviceInformationCustomPairing, DevicePairingKinds, DevicePairingProtectionLevel,
            DevicePairingRequestedEventArgs, DevicePairingResultStatus, DeviceUnpairingResultStatus,
        },
        Radios::{Radio, RadioState},
    },
    Foundation::TypedEventHandler,
    Storage::Streams::{DataReader, IBuffer},
};
use windows_future::{AsyncStatus, IAsyncOperation};

/// GAP appearance "HID Mouse", for the paired list.
const APPEARANCE_MOUSE: u16 = 0x03C2;
const PAIR_SECS: u64 = 30;

#[derive(Clone)]
pub struct BtHandle {
    events: Sender<Event>,
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    watcher: Option<BluetoothLEAdvertisementWatcher>,
    scan: Scan,
    /// Watched paired mice and their ConnectionStatusChanged tokens (B5). Holding the device keeps the subscription alive.
    links: Vec<(BluetoothLEDevice, i64)>,
    link_states: Links,
    /// The adapter's radio and its StateChanged token: Bluetooth switched off/on in Windows.
    radio: Option<(Radio, i64)>,
    /// The Battery Level characteristic we're subscribed to (B6), and its ValueChanged token.
    battery: Option<(GattCharacteristic, i64)>,
    /// Bumped on every battery start/stop, so a slow read that finishes after a disconnect is dropped.
    battery_gen: u64,
    /// Last value sent, so `battery` goes out only on change. `None` = unknown ("—").
    battery_last: Option<u8>,
    /// Bumped on every start, so an old auto-stop timer can't stop a newer search.
    generation: u64,
}

pub fn start(events: Sender<Event>) -> BtHandle {
    BtHandle { events, inner: Arc::default() }
}

/// Runs WinRT work on a fresh thread and waits for it: blocking `.get()` must not run on the UI (STA) thread.
fn on_mta<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    thread::spawn(f).join().ok()
}

fn adapter_state() -> windows::core::Result<AdapterState> {
    let adapter = match BluetoothAdapter::GetDefaultAsync()?.get() {
        Ok(a) => a,
        Err(_) => return Ok(AdapterState::Missing), // no default adapter
    };
    if !adapter.IsLowEnergySupported()? {
        return Ok(AdapterState::Missing);
    }
    let radio = adapter.GetRadioAsync()?.get()?;
    Ok(if radio.State()? == RadioState::On { AdapterState::On } else { AdapterState::Off })
}

/// The Bluetooth base UUID (0000xxxx-0000-1000-8000-00805F9B34FB) → its 16-bit short form.
fn short_uuid(g: &GUID) -> Option<u16> {
    let base = GUID::from_u128(0x0000_0000_0000_1000_8000_00805F9B34FB);
    (g.data2 == base.data2 && g.data3 == base.data3 && g.data4 == base.data4 && g.data1 <= 0xFFFF).then(|| g.data1 as u16)
}

fn read_advert(args: &BluetoothLEAdvertisementReceivedEventArgs) -> windows::core::Result<Advert> {
    let adv = args.Advertisement()?;
    let mut out = Advert {
        address: args.BluetoothAddress()?,
        rssi: args.RawSignalStrengthInDBm()?,
        name: Some(adv.LocalName()?.to_string()).filter(|n| !n.is_empty()),
        ..Default::default()
    };
    for g in adv.ServiceUuids()? {
        if let Some(u) = short_uuid(&g) {
            out.uuids16.push(u);
        }
    }
    for section in adv.DataSections()? {
        if section.DataType()? == AD_APPEARANCE {
            let buf = section.Data()?;
            let mut bytes = vec![0u8; buf.Length()? as usize];
            DataReader::FromBuffer(&buf)?.ReadBytes(&mut bytes)?;
            out.appearance = appearance(&bytes);
        }
    }
    Ok(out)
}

impl BtHandle {
    pub fn adapter(&self) -> AdapterState {
        on_mta(|| adapter_state().unwrap_or(AdapterState::Missing)).unwrap_or(AdapterState::Missing)
    }

    pub fn scan(&self, on: bool) -> Result<(), String> {
        if !on {
            if !stop_watcher(&self.inner, &self.events, None) {
                // Nothing was running: still tell the UI, so a stale "Stop" button always recovers.
                let _ = self.events.send(Event::BtScan { active: false });
            }
            return Ok(());
        }
        let state = self.adapter();
        let _ = self.events.send(Event::BtAdapter { state });
        match state {
            AdapterState::Missing => return Err("No Bluetooth LE adapter found.".into()),
            AdapterState::Off => return Err("Bluetooth is off.".into()),
            AdapterState::On => {}
        }
        stop_watcher(&self.inner, &self.events, None); // restart cleanly if a search is already running
        let (events, inner) = (self.events.clone(), self.inner.clone());
        let generation = on_mta(move || -> windows::core::Result<u64> {
            let mut g = inner.lock().unwrap();
            g.scan.clear();
            g.generation += 1;
            let mine = g.generation;
            let watcher = BluetoothLEAdvertisementWatcher::new()?;
            watcher.SetScanningMode(BluetoothLEScanningMode::Active)?; // active = also get the scan response, which has the name
            let (ev, st) = (events.clone(), inner.clone());
            watcher.Received(&TypedEventHandler::new(
                move |_: Ref<BluetoothLEAdvertisementWatcher>, args: Ref<BluetoothLEAdvertisementReceivedEventArgs>| {
                    if let Some(args) = args.as_ref() {
                        if let Ok(adv) = read_advert(args) {
                            if let Some(found) = st.lock().unwrap().scan.update(&adv) {
                                let _ = ev.send(Event::BtFound(found));
                            }
                        }
                    }
                    Ok(())
                },
            ))?;
            // Only for stops we didn't ask for (radio switched off, adapter removed). Our own stops report
            // themselves in `stop_watcher`: once we drop the watcher, Windows may never deliver this event.
            let (ev, st) = (events.clone(), inner.clone());
            watcher.Stopped(&TypedEventHandler::new(
                move |_: Ref<BluetoothLEAdvertisementWatcher>, _: Ref<BluetoothLEAdvertisementWatcherStoppedEventArgs>| {
                    let mut g = st.lock().unwrap();
                    if g.generation == mine && g.watcher.take().is_some() {
                        let _ = ev.send(Event::BtScan { active: false });
                    }
                    Ok(())
                },
            ))?;
            // Unlocked while starting: if Windows raised Stopped synchronously, its handler must not wait on us.
            drop(g);
            watcher.Start()?;
            let mut g = inner.lock().unwrap();
            if g.generation == mine {
                g.watcher = Some(watcher);
            }
            Ok(mine)
        })
        .ok_or("the Bluetooth search thread failed")?
        .map_err(|e| format!("Couldn't start the Bluetooth search: {}", e.message()))?;
        let _ = self.events.send(Event::BtScan { active: true });

        let (inner, events) = (self.inner.clone(), self.events.clone());
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(SCAN_SECS));
            stop_watcher(&inner, &events, Some(generation));
        });
        Ok(())
    }

    pub fn open_settings(&self) -> Result<(), String> {
        std::process::Command::new("explorer.exe").arg("ms-settings:bluetooth").spawn().map(|_| ()).map_err(|e| e.to_string())
    }

    /// Pairs with "just works" (confirm only), as mice do. A PIN request fails with a message instead
    /// of being answered for the user. On failure, a half-made pairing is removed.
    pub fn pair(&self, address: &str) -> Result<Paired, String> {
        let addr = parse_address(address).ok_or_else(|| format!("'{address}' isn't a Bluetooth address"))?;
        stop_watcher(&self.inner, &self.events, None); // the radio pairs more reliably without an active scan
        on_mta(move || pair_blocking(addr)).ok_or("the Bluetooth pairing thread failed")?
    }

    pub fn forget(&self, address: &str) -> Result<(), String> {
        let addr = parse_address(address).ok_or_else(|| format!("'{address}' isn't a Bluetooth address"))?;
        on_mta(move || forget_blocking(addr)).ok_or("the Bluetooth thread failed")?
    }

    pub fn paired(&self) -> Vec<PairedMouse> {
        on_mta(|| paired_mice().unwrap_or_default()).unwrap_or_default()
    }

    /// Watches these paired mice (B5): sends `link {bluetooth, state, address}` now and on every change,
    /// and `btAdapter` when Bluetooth is switched off or on. Replaces the previous watch list.
    pub fn watch(&self, addresses: &[String]) {
        let addrs: Vec<u64> = addresses.iter().filter_map(|a| parse_address(a)).collect();
        let (events, inner) = (self.events.clone(), self.inner.clone());
        // In the background: lookups can take seconds and must not hold up startup or a command.
        thread::spawn(move || {
            static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
            let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
            watch_blocking(addrs, events, inner);
        });
    }
}

fn link_event(addr: u64, state: LinkState) -> Event {
    Event::Link { bus: Bus::Bluetooth, state, address: Some(format_address(addr)) }
}

fn report_link(dev: &BluetoothLEDevice, addr: u64, events: &Sender<Event>, inner: &Arc<Mutex<Inner>>) {
    let connected = dev.ConnectionStatus().is_ok_and(|s| s == BluetoothConnectionStatus::Connected);
    let paired = dev.DeviceInformation().and_then(|i| i.Pairing()).and_then(|p| p.IsPaired()).unwrap_or(false);
    let state = inner.lock().unwrap().link_states.update(addr, paired, connected);
    if let Some(state) = state {
        let _ = events.send(link_event(addr, state));
        if state == LinkState::Connected {
            let (d, ev, st) = (dev.clone(), events.clone(), inner.clone());
            thread::spawn(move || start_battery(d, ev, st)); // GATT reads take a moment: not on the event thread
            let (d, ev) = (dev.clone(), events.clone());
            thread::spawn(move || {
                if let Some(info) = read_info(&d, addr) {
                    let _ = ev.send(info);
                }
            });
        } else {
            stop_battery(inner, events); // asleep or off: "—", never the last number
        }
    }
}

/// Device Information (`180A`): manufacturer `2A29`, firmware `2A26` or else software revision `2A28`
/// (B0: the N230 has `BEKEN` and software `5.3.5.8`, no firmware string). `None` if the service is unreadable.
fn read_info(dev: &BluetoothLEDevice, addr: u64) -> Option<Event> {
    let services = wait(
        dev.GetGattServicesForUuidWithCacheModeAsync(GattServiceUuids::DeviceInformation().ok()?, BluetoothCacheMode::Uncached).ok()?,
        10,
        "reading the device information",
    )
    .ok()?;
    let service = services.Services().ok()?.GetAt(0).ok()?;
    let text = |uuid: windows::core::Result<GUID>| -> Option<String> {
        let chars = wait(service.GetCharacteristicsForUuidAsync(uuid.ok()?).ok()?, 5, "reading the device information").ok()?;
        let read = wait(chars.Characteristics().ok()?.GetAt(0).ok()?.ReadValueAsync().ok()?, 5, "reading the device information").ok()?;
        (read.Status().ok()? == GattCommunicationStatus::Success).then_some(())?;
        let s = String::from_utf8_lossy(&buffer_bytes(&read.Value().ok()?).ok()?).trim_end_matches('\0').trim().to_string();
        (!s.is_empty()).then_some(s)
    };
    let manufacturer = text(GattCharacteristicUuids::ManufacturerNameString());
    let firmware = text(GattCharacteristicUuids::FirmwareRevisionString()).or_else(|| text(GattCharacteristicUuids::SoftwareRevisionString()));
    Some(Event::BtInfo { address: format_address(addr), manufacturer, firmware })
}

/// Sends `battery` if the value changed.
fn set_battery(inner: &Arc<Mutex<Inner>>, events: &Sender<Event>, percent: Option<u8>) {
    let mut g = inner.lock().unwrap();
    if g.battery_last != percent {
        g.battery_last = percent;
        let _ = events.send(Event::Battery { percent });
    }
}

fn stop_battery(inner: &Arc<Mutex<Inner>>, events: &Sender<Event>) {
    let old = {
        let mut g = inner.lock().unwrap();
        g.battery_gen += 1;
        g.battery.take()
    };
    if let Some((ch, token)) = old {
        let _ = ch.RemoveValueChanged(token);
    }
    set_battery(inner, events, None);
}

/// The standard Battery Service (B0: `180F`/`2A19`, Read + Notify; Windows lets apps use it on a HID
/// mouse, only `1812` is blocked). Reads once, then subscribes so changes arrive without polling.
fn start_battery(dev: BluetoothLEDevice, events: Sender<Event>, inner: Arc<Mutex<Inner>>) {
    let mine = {
        let mut g = inner.lock().unwrap();
        g.battery_gen += 1;
        g.battery_gen
    };
    let current = |st: &Arc<Mutex<Inner>>| st.lock().unwrap().battery_gen == mine;
    let result = (|| -> Result<(), String> {
        let e = |e: windows::core::Error| e.message();
        let services = wait(
            dev.GetGattServicesForUuidWithCacheModeAsync(GattServiceUuids::Battery().map_err(e)?, BluetoothCacheMode::Uncached).map_err(e)?,
            10,
            "finding the battery service",
        )?;
        let service = services.Services().map_err(e)?.GetAt(0).map_err(e)?;
        let chars = wait(service.GetCharacteristicsForUuidAsync(GattCharacteristicUuids::BatteryLevel().map_err(e)?).map_err(e)?, 10, "finding the battery level")?;
        let ch = chars.Characteristics().map_err(e)?.GetAt(0).map_err(e)?;
        let read = wait(ch.ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached).map_err(e)?, 10, "reading the battery")?;
        if read.Status().map_err(e)? != GattCommunicationStatus::Success {
            return Err("battery level unreadable".into());
        }
        let percent = *buffer_bytes(&read.Value().map_err(e)?)?.first().ok_or("empty battery value")?;
        if !current(&inner) {
            return Ok(()); // disconnected while we were reading
        }
        set_battery(&inner, &events, Some(percent.min(100)));

        let (ev, st) = (events.clone(), inner.clone());
        let token = ch
            .ValueChanged(&TypedEventHandler::new(move |_: Ref<GattCharacteristic>, args: Ref<GattValueChangedEventArgs>| {
                if let Some(a) = args.as_ref() {
                    if let Some(p) = buffer_bytes(&a.CharacteristicValue()?).ok().and_then(|b| b.first().copied()) {
                        set_battery(&st, &ev, Some(p.min(100)));
                    }
                }
                Ok(())
            }))
            .map_err(e)?;
        let _ = wait(
            ch.WriteClientCharacteristicConfigurationDescriptorAsync(GattClientCharacteristicConfigurationDescriptorValue::Notify).map_err(e)?,
            10,
            "subscribing to the battery",
        ); // without notifications we still have the first read; the next connect reads again
        let mut g = inner.lock().unwrap();
        if g.battery_gen == mine {
            g.battery = Some((ch, token));
        } else {
            let _ = ch.RemoveValueChanged(token);
        }
        Ok(())
    })();
    if let Err(msg) = result {
        if current(&inner) {
            let _ = events.send(Event::Error { message: format!("Battery: {msg}") });
        }
    }
}

fn watch_blocking(addrs: Vec<u64>, events: Sender<Event>, inner: Arc<Mutex<Inner>>) {
    // Unsubscribe the old list first (outside the lock: handlers lock it too).
    let old = std::mem::take(&mut inner.lock().unwrap().links);
    for (dev, token) in old {
        let _ = dev.RemoveConnectionStatusChanged(token);
    }
    stop_battery(&inner, &events); // the connected mouse in the new list starts it again
    inner.lock().unwrap().link_states.keep(&addrs);

    let mut links = Vec::new();
    for addr in addrs {
        let Ok(dev) = le_device(addr) else {
            // Not known to Windows (forgotten elsewhere): report it as disconnected once.
            if let Some(state) = inner.lock().unwrap().link_states.update(addr, false, false) {
                let _ = events.send(link_event(addr, state));
            }
            continue;
        };
        let (ev, st) = (events.clone(), inner.clone());
        let token = dev.ConnectionStatusChanged(&TypedEventHandler::new(move |d: Ref<BluetoothLEDevice>, _: Ref<IInspectable>| {
            if let Some(d) = d.as_ref() {
                report_link(d, addr, &ev, &st);
            }
            Ok(())
        }));
        report_link(&dev, addr, &events, &inner);
        if let Ok(token) = token {
            links.push((dev, token));
        }
    }
    inner.lock().unwrap().links = links;

    if inner.lock().unwrap().radio.is_none() {
        let radio = BluetoothAdapter::GetDefaultAsync()
            .map_err(|e| e.message())
            .and_then(|op| wait(op, 5, "finding the adapter"))
            .and_then(|a| a.GetRadioAsync().map_err(|e| e.message()))
            .and_then(|op| wait(op, 5, "finding the radio"));
        if let Ok(radio) = radio {
            let (ev, st) = (events.clone(), inner.clone());
            let token = radio.StateChanged(&TypedEventHandler::new(move |r: Ref<Radio>, _: Ref<IInspectable>| {
                if let Some(r) = r.as_ref() {
                    let state = if r.State()? == RadioState::On { AdapterState::On } else { AdapterState::Off };
                    let _ = ev.send(Event::BtAdapter { state });
                    if state == AdapterState::Off {
                        // Windows doesn't raise ConnectionStatusChanged while the radio is off (seen 2026-11-08:
                        // the link only changed when it came back on), so every watched mouse drops here.
                        {
                            let mut g = st.lock().unwrap();
                            let addrs: Vec<u64> = g.links.iter().filter_map(|(d, _)| d.BluetoothAddress().ok()).collect();
                            for addr in addrs {
                                if let Some(s) = g.link_states.update(addr, true, false) {
                                    let _ = ev.send(link_event(addr, s));
                                }
                            }
                        }
                        stop_battery(&st, &ev);
                    }
                }
                Ok(())
            }));
            if let Ok(token) = token {
                inner.lock().unwrap().radio = Some((radio, token));
            }
        }
    }
}

/// Waits for a WinRT async operation without `.get()`, so it can give up: a mouse that leaves pairing
/// mode mid-way must not hang the app. Cancels the operation on timeout.
fn wait<T: RuntimeType + 'static>(op: IAsyncOperation<T>, secs: u64, what: &str) -> Result<T, String> {
    let end = Instant::now() + Duration::from_secs(secs);
    loop {
        match op.Status().map_err(|e| e.message())? {
            AsyncStatus::Started if Instant::now() < end => thread::sleep(Duration::from_millis(100)),
            AsyncStatus::Started => {
                let _ = op.Cancel();
                return Err(format!("Timed out {what}. Move the mouse to wake it and try again."));
            }
            AsyncStatus::Completed => return op.GetResults().map_err(|e| e.message()),
            _ => return Err(format!("Failed {what}: {}", op.ErrorCode().map(|c| c.message()).unwrap_or_default())),
        }
    }
}

/// The LE device at this address. Mice use random addresses (B0), so that type is tried first.
fn le_device(addr: u64) -> Result<BluetoothLEDevice, String> {
    for kind in [BluetoothAddressType::Random, BluetoothAddressType::Public] {
        if let Ok(op) = BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync(addr, kind) {
            if let Ok(d) = wait(op, 10, "finding the mouse") {
                return Ok(d);
            }
        }
    }
    Err("The mouse isn't reachable. Put it in pairing mode and search again.".into())
}

fn pairing_message(s: DevicePairingResultStatus) -> String {
    match s {
        DevicePairingResultStatus::AuthenticationTimeout => "The mouse didn't answer in time. Is it still in pairing mode?".into(),
        DevicePairingResultStatus::ConnectionRejected | DevicePairingResultStatus::RejectedByHandler => {
            "The mouse refused the pairing. Put it in pairing mode again and retry.".into()
        }
        DevicePairingResultStatus::RequiredHandlerNotRegistered | DevicePairingResultStatus::PairingCanceled => {
            "This device asks for a PIN. Pair it in the Windows Bluetooth settings instead.".into()
        }
        DevicePairingResultStatus::NotReadyToPair => "The mouse isn't ready to pair. Put it in pairing mode and search again.".into(),
        other => format!("Pairing failed (Windows status {}). Put the mouse in pairing mode and retry.", other.0),
    }
}

fn pair_blocking(addr: u64) -> Result<Paired, String> {
    let dev = le_device(addr)?;
    let name = dev.Name().map(|n| n.to_string()).unwrap_or_default();
    let pairing = dev.DeviceInformation().and_then(|i| i.Pairing()).map_err(|e| e.message())?;
    if !pairing.IsPaired().unwrap_or(false) {
        if !pairing.CanPair().unwrap_or(false) {
            return Err("Windows can't pair this device right now. Is it in pairing mode?".into());
        }
        let custom: DeviceInformationCustomPairing = pairing.Custom().map_err(|e| e.message())?;
        let token = custom
            .PairingRequested(&TypedEventHandler::new(
                |_: Ref<DeviceInformationCustomPairing>, args: Ref<DevicePairingRequestedEventArgs>| {
                    if let Some(a) = args.as_ref() {
                        if a.PairingKind()? == DevicePairingKinds::ConfirmOnly {
                            a.Accept()?; // "just works": nothing for the user to compare or type
                        }
                    }
                    Ok(())
                },
            ))
            .map_err(|e| e.message())?;
        let result = custom
            .PairWithProtectionLevelAsync(DevicePairingKinds::ConfirmOnly, DevicePairingProtectionLevel::Default)
            .map_err(|e| e.message())
            .and_then(|op| wait(op, PAIR_SECS, "pairing"));
        let _ = custom.RemovePairingRequested(token);
        let status = result.and_then(|r| r.Status().map_err(|e| e.message()));
        match status {
            Ok(DevicePairingResultStatus::Paired | DevicePairingResultStatus::AlreadyPaired) => {}
            failed => {
                if pairing.IsPaired().unwrap_or(false) {
                    let _ = pairing.UnpairAsync().map(|op| wait(op, 10, "undoing the pairing"));
                }
                return Err(match failed {
                    Ok(s) => pairing_message(s),
                    Err(e) => e,
                });
            }
        }
    }
    Ok(Paired { address: format_address(addr), name, id: read_pnp_id(&dev).ok() })
}

/// The Device Information service's PnP ID (0x2A50): the VID/PID the mouse has on this link (B0: 25A7:FAA2).
fn read_pnp_id(dev: &BluetoothLEDevice) -> Result<DeviceId, String> {
    let e = |e: windows::core::Error| e.message();
    let services = wait(
        dev.GetGattServicesForUuidWithCacheModeAsync(GattServiceUuids::DeviceInformation().map_err(e)?, BluetoothCacheMode::Uncached).map_err(e)?,
        10,
        "reading the device information",
    )?;
    let service = services.Services().map_err(e)?.GetAt(0).map_err(e)?;
    let chars = wait(service.GetCharacteristicsForUuidAsync(GattCharacteristicUuids::PnpId().map_err(e)?).map_err(e)?, 10, "reading the PnP ID")?;
    let read = wait(chars.Characteristics().map_err(e)?.GetAt(0).map_err(e)?.ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached).map_err(e)?, 10, "reading the PnP ID")?;
    if read.Status().map_err(e)? != GattCommunicationStatus::Success {
        return Err("PnP ID unreadable".into());
    }
    DeviceId::from_pnp_id(&buffer_bytes(&read.Value().map_err(e)?)?).ok_or("bad PnP ID".into())
}

fn buffer_bytes(buf: &IBuffer) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0u8; buf.Length().map_err(|e| e.message())? as usize];
    DataReader::FromBuffer(buf).and_then(|r| r.ReadBytes(&mut bytes)).map_err(|e| e.message())?;
    Ok(bytes)
}

fn forget_blocking(addr: u64) -> Result<(), String> {
    let dev = le_device(addr)?;
    let pairing = dev.DeviceInformation().and_then(|i| i.Pairing()).map_err(|e| e.message())?;
    if !pairing.IsPaired().unwrap_or(false) {
        return Ok(());
    }
    let r = wait(pairing.UnpairAsync().map_err(|e| e.message())?, 15, "forgetting the mouse")?;
    match r.Status().map_err(|e| e.message())? {
        DeviceUnpairingResultStatus::Unpaired | DeviceUnpairingResultStatus::AlreadyUnpaired => Ok(()),
        s => Err(format!("Windows couldn't forget the mouse (status {}).", s.0)),
    }
}

/// Paired LE devices that are mice (appearance 0x03C2) or named like the N230.
fn paired_mice() -> Result<Vec<PairedMouse>, String> {
    let e = |e: windows::core::Error| e.message();
    let selector: HSTRING = BluetoothLEDevice::GetDeviceSelectorFromPairingState(true).map_err(e)?;
    let infos = wait(DeviceInformation::FindAllAsyncAqsFilter(&selector).map_err(e)?, 10, "listing paired devices")?;
    let mut out = Vec::new();
    for info in infos {
        let Ok(op) = BluetoothLEDevice::FromIdAsync(&info.Id().map_err(e)?) else { continue };
        let Ok(dev) = wait(op, 5, "opening a paired device") else { continue };
        let name = dev.Name().map(|n| n.to_string()).unwrap_or_default();
        let mouse = dev.Appearance().and_then(|a| a.RawValue()).is_ok_and(|a| a == APPEARANCE_MOUSE);
        if mouse || is_n230(&name) {
            out.push(PairedMouse {
                address: format_address(dev.BluetoothAddress().map_err(e)?),
                n230: is_n230(&name),
                name,
                connected: dev.ConnectionStatus().is_ok_and(|s| s == BluetoothConnectionStatus::Connected),
            });
        }
    }
    Ok(out)
}

/// Stops the current search and sends `btScan {active: false}` itself. `only`: stop only if that
/// search is still the current one (the auto-stop timer). Returns whether a search was running.
fn stop_watcher(inner: &Arc<Mutex<Inner>>, events: &Sender<Event>, only: Option<u64>) -> bool {
    let watcher = {
        let mut g = inner.lock().unwrap();
        if only.is_some_and(|n| n != g.generation) {
            return false;
        }
        g.watcher.take() // taken first, so the Stopped handler sees it's gone and stays quiet
    };
    let Some(w) = watcher else { return false };
    let _ = on_mta(move || {
        let _ = w.Stop();
    });
    let _ = events.send(Event::BtScan { active: false });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_uuids() {
        assert_eq!(short_uuid(&GUID::from_u128(0x0000_1812_0000_1000_8000_00805F9B34FB)), Some(0x1812));
        assert_eq!(short_uuid(&GUID::from_u128(0x0000_FFF0_0000_1000_8000_00805F9B34FB)), Some(0xFFF0));
        assert_eq!(short_uuid(&GUID::from_u128(0x1234_5678_0000_1000_8000_00805F9B34FB)), None, "32-bit, not 16");
        assert_eq!(short_uuid(&GUID::from_u128(0x0000_1812_0000_1000_8000_000000000000)), None, "not the base UUID");
    }
}
