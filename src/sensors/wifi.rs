//! CoreWLAN link quality/signal — no `objc2-core-wlan` binding crate exists
//! (CoreWLAN isn't in the objc2 framework-bindings family), so this talks to
//! `CWWiFiClient`/`CWInterface` directly via the Objective-C runtime.
//! Framework is linked in `build.rs`.
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::msg_send;
use std::ffi::CStr;

#[derive(Debug, Clone, Copy, Default)]
pub struct WifiReading {
    pub rssi_dbm: Option<i64>,
    pub noise_dbm: Option<i64>,
    /// Heuristic mapping (-100 dBm -> 0%, -50 dBm or better -> 100%),
    /// not an Apple-published formula — CoreWLAN exposes no direct
    /// "quality %" API.
    pub link_quality_percent: Option<f64>,
}

fn class_named(name: &str) -> Option<&'static AnyClass> {
    let cname = std::ffi::CString::new(name).ok()?;
    let cstr: &CStr = &cname;
    AnyClass::get(cstr)
}

pub fn read_wifi() -> WifiReading {
    unsafe {
        let Some(client_cls) = class_named("CWWiFiClient") else {
            return WifiReading::default();
        };
        let client: Option<Retained<AnyObject>> = msg_send![client_cls, sharedWiFiClient];
        let Some(client) = client else {
            return WifiReading::default();
        };
        let iface: Option<Retained<AnyObject>> = msg_send![&*client, interface];
        let Some(iface) = iface else {
            return WifiReading::default();
        };
        let rssi: i64 = msg_send![&*iface, rssiValue];
        let noise: i64 = msg_send![&*iface, noiseMeasurement];

        // rssi == 0 is CoreWLAN's "no association" sentinel.
        if rssi == 0 {
            return WifiReading::default();
        }

        let quality = ((rssi as f64 + 100.0) * 2.0).clamp(0.0, 100.0);

        WifiReading {
            rssi_dbm: Some(rssi),
            noise_dbm: Some(noise),
            link_quality_percent: Some(quality),
        }
    }
}
