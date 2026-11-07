//! Device identity: which bus a mouse is on and its VID/PID, parsed from what each OS gives us.
//! The one place that reads Windows device paths, Linux sysfs ids and the Bluetooth PnP ID.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Bus {
    Usb,
    Bluetooth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceId {
    pub bus: Bus,
    pub vid: u16,
    pub pid: u16,
}

/// Linux `input_id.bustype` values (linux/input.h).
const BUS_USB: u16 = 0x03;
const BUS_BLUETOOTH: u16 = 0x05;

fn hex16(s: &str) -> Option<u16> {
    if s.len() == 4 && s.bytes().all(|b| b.is_ascii_hexdigit()) { u16::from_str_radix(s, 16).ok() } else { None }
}

impl DeviceId {
    /// A Windows Raw Input / HID interface path.
    /// - USB: `\\?\HID#VID_3554&PID_FC01&MI_00&Col01#…`
    /// - Bluetooth Classic: `\\?\HID#{00001124-…}_VID&0002046d_PID&b01a&Col01#…`
    /// - Bluetooth LE: `\\?\HID#{00001812-…}_Dev_VID&0225a7_PID&faa2_REV&0508_fe46e9240491&Col02#…`
    ///
    /// On Bluetooth the VID is prefixed by its source (`01` Bluetooth SIG, `02` USB-IF); only the last 4 digits are the VID.
    pub fn from_windows_path(path: &str) -> Option<Self> {
        let p = path.to_ascii_uppercase();
        let pid_after = |rest: &str| hex16(rest.get(..4)?);

        if let Some(i) = p.find("VID_") {
            let rest = &p[i + 4..];
            let vid = hex16(rest.get(..4)?)?;
            let pid = pid_after(rest.get(4..)?.strip_prefix("&PID_")?)?;
            return Some(Self { bus: Bus::Usb, vid, pid });
        }
        // Bluetooth: the HID service UUID comes first, then `VID&<source><vid>_PID&<pid>`.
        if !(p.contains("{00001124-") || p.contains("{00001812-")) {
            return None;
        }
        let i = p.find("VID&")?;
        let rest = &p[i + 4..];
        let end = rest.find('_')?;
        let vid = hex16(rest[..end].get(end.checked_sub(4)?..)?)?;
        let pid = pid_after(rest[end..].strip_prefix("_PID&")?)?;
        Some(Self { bus: Bus::Bluetooth, vid, pid })
    }

    /// Linux `/sys/class/input/eventN/device/id/{bustype,vendor,product}`. Other buses are not ours.
    pub fn from_linux_ids(bustype: u16, vendor: u16, product: u16) -> Option<Self> {
        let bus = match bustype {
            BUS_USB => Bus::Usb,
            BUS_BLUETOOTH => Bus::Bluetooth,
            _ => return None,
        };
        Some(Self { bus, vid: vendor, pid: product })
    }

    /// A Linux HID device name from `/sys/bus/hid/devices/`, e.g. `0005:25A7:FAA2.0003`.
    pub fn from_linux_hid_name(name: &str) -> Option<Self> {
        let mut parts = name.split(['.', ':']);
        let bus = u16::from_str_radix(parts.next()?, 16).ok()?;
        let vid = hex16(parts.next()?)?;
        let pid = hex16(parts.next()?)?;
        Self::from_linux_ids(bus, vid, pid)
    }

    /// The Bluetooth LE PnP ID characteristic (0x2A50): source, VID, PID, version, little-endian.
    /// Source 1 = Bluetooth SIG, 2 = USB-IF; anything else is invalid.
    pub fn from_pnp_id(b: &[u8]) -> Option<Self> {
        if b.len() != 7 || !(b[0] == 1 || b[0] == 2) {
            return None;
        }
        let vid = u16::from_le_bytes([b[1], b[2]]);
        let pid = u16::from_le_bytes([b[3], b[4]]);
        Some(Self { bus: Bus::Bluetooth, vid, pid })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const N230_BT: DeviceId = DeviceId { bus: Bus::Bluetooth, vid: 0x25A7, pid: 0xFAA2 };

    #[test]
    fn windows_paths() {
        assert_eq!(
            DeviceId::from_windows_path(r"\\?\HID#VID_3554&PID_FC01&MI_00&Col01#8&1d0b0c4a&0&0000#{378de44c}"),
            Some(DeviceId { bus: Bus::Usb, vid: 0x3554, pid: 0xFC01 })
        );
        // Captured from the real N230 over Bluetooth LE (B0, 2026-11-07).
        assert_eq!(
            DeviceId::from_windows_path(r"\\?\HID#{00001812-0000-1000-8000-00805f9b34fb}_Dev_VID&0225a7_PID&faa2_REV&0508_fe46e9240491&Col02#9&39f0bed3&0&0001#{378de44c-56ef-11d1-bc8c-00a0c91405dd}"),
            Some(N230_BT)
        );
        assert_eq!(
            DeviceId::from_windows_path(r"\\?\HID#{00001124-0000-1000-8000-00805f9b34fb}_VID&0002046d_PID&b01a&Col01#9&3"),
            Some(DeviceId { bus: Bus::Bluetooth, vid: 0x046D, pid: 0xB01A })
        );
        for bad in ["", r"\\?\HID#VID_35&PID_FC01", r"\\?\HID#VID_3554&PID_", r"\\?\ACPI#PNP0F13#4&1", r"\\?\HID#{00001812-0000}_Dev_VID&02_PID&faa2"] {
            assert_eq!(DeviceId::from_windows_path(bad), None, "{bad}");
        }
    }

    #[test]
    fn linux_ids() {
        assert_eq!(DeviceId::from_linux_ids(0x05, 0x25A7, 0xFAA2), Some(N230_BT));
        assert_eq!(DeviceId::from_linux_ids(0x03, 0x3554, 0xFC01).unwrap().bus, Bus::Usb);
        assert_eq!(DeviceId::from_linux_ids(0x06, 0x25A7, 0xFAA2), None, "BUS_VIRTUAL (our own uinput device) is never ours");
        assert_eq!(DeviceId::from_linux_hid_name("0005:25A7:FAA2.0003"), Some(N230_BT));
        assert_eq!(DeviceId::from_linux_hid_name("0003:3554:FC01.0001").unwrap().bus, Bus::Usb);
        assert_eq!(DeviceId::from_linux_hid_name("0005:25A7"), None);
    }

    #[test]
    fn pnp_id() {
        // Read from the N230's 0x2A50 characteristic in B0.
        assert_eq!(DeviceId::from_pnp_id(&[0x02, 0xA7, 0x25, 0xA2, 0xFA, 0x08, 0x05]), Some(N230_BT));
        assert_eq!(DeviceId::from_pnp_id(&[0x00, 0xA7, 0x25, 0xA2, 0xFA, 0x08, 0x05]), None, "unknown source");
        assert_eq!(DeviceId::from_pnp_id(&[0x02, 0xA7, 0x25]), None);
    }

    #[test]
    fn same_mouse_from_every_source() {
        let w = DeviceId::from_windows_path(r"\\?\HID#{00001812-0000-1000-8000-00805f9b34fb}_Dev_VID&0225a7_PID&faa2_REV&0508_x&Col02#a");
        let l = DeviceId::from_linux_hid_name("0005:25A7:FAA2.000A");
        let g = DeviceId::from_pnp_id(&[0x02, 0xA7, 0x25, 0xA2, 0xFA, 0x08, 0x05]);
        assert!(w.is_some() && w == l && l == g);
    }
}
