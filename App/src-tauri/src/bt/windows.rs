//! Windows: WinRT `BluetoothLEAdvertisementWatcher` for searching, `BluetoothAdapter` + `Radio` for adapter state.
//! WinRT calls run on our own threads (implicit MTA), never on the UI thread or the input hook thread.

use super::{appearance, AdapterState, Advert, Scan, AD_APPEARANCE, SCAN_SECS};
use crate::event::Event;
use std::{
    sync::{mpsc::Sender, Arc, Mutex},
    thread,
    time::Duration,
};
use windows::{
    core::{Ref, GUID},
    Devices::{
        Bluetooth::{
            Advertisement::{
                BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementWatcher,
                BluetoothLEAdvertisementWatcherStoppedEventArgs, BluetoothLEScanningMode,
            },
            BluetoothAdapter,
        },
        Radios::RadioState,
    },
    Foundation::TypedEventHandler,
    Storage::Streams::DataReader,
};

pub struct BtHandle {
    events: Sender<Event>,
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    watcher: Option<BluetoothLEAdvertisementWatcher>,
    scan: Scan,
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
