//! Battery via `IOPSCopyPowerSourcesInfo`/`IOPSCopyPowerSourcesList`/
//! `IOPSGetPowerSourceDescription` (`IOKit/ps/IOPowerSources.h`) — a small
//! standalone header not covered by registry-focused IOKit crates, so the 3
//! functions are hand-declared here. Framework linked in `build.rs`.
use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation_sys::base::{kCFAllocatorDefault, CFGetTypeID, CFRelease, CFTypeRef};
use core_foundation_sys::dictionary::{CFDictionaryGetValue, CFDictionaryRef};
use core_foundation_sys::number::{
    kCFNumberSInt64Type, CFBooleanGetTypeID, CFBooleanGetValue, CFBooleanRef, CFNumberGetTypeID, CFNumberGetValue,
    CFNumberRef,
};
use core_foundation_sys::string::{
    kCFStringEncodingUTF8, CFStringCreateWithCString, CFStringGetCString, CFStringGetTypeID, CFStringRef,
};
use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::os::raw::c_void;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
    fn IOPSCopyPowerSourcesList(blob: CFTypeRef) -> CFArrayRef;
    fn IOPSGetPowerSourceDescription(blob: CFTypeRef, ps: CFTypeRef) -> CFDictionaryRef;
}

fn cfstring(s: &str) -> Option<CFStringRef> {
    let c = CString::new(s).ok()?;
    let r = unsafe { CFStringCreateWithCString(kCFAllocatorDefault, c.as_ptr(), kCFStringEncodingUTF8) };
    if r.is_null() {
        None
    } else {
        Some(r)
    }
}

unsafe fn dict_lookup(dict: CFDictionaryRef, key: &str) -> *const c_void {
    let Some(cfkey) = cfstring(key) else {
        return std::ptr::null();
    };
    let value = unsafe { CFDictionaryGetValue(dict, cfkey as *const c_void) };
    unsafe { CFRelease(cfkey as CFTypeRef) };
    value
}

unsafe fn dict_get_i64(dict: CFDictionaryRef, key: &str) -> Option<i64> {
    unsafe {
        let value = dict_lookup(dict, key);
        if value.is_null() || CFGetTypeID(value) != CFNumberGetTypeID() {
            return None;
        }
        let mut out: i64 = 0;
        let ok = CFNumberGetValue(value as CFNumberRef, kCFNumberSInt64Type, &mut out as *mut _ as *mut c_void);
        if ok {
            Some(out)
        } else {
            None
        }
    }
}

unsafe fn dict_get_bool(dict: CFDictionaryRef, key: &str) -> Option<bool> {
    unsafe {
        let value = dict_lookup(dict, key);
        if value.is_null() || CFGetTypeID(value) != CFBooleanGetTypeID() {
            return None;
        }
        Some(CFBooleanGetValue(value as CFBooleanRef))
    }
}

unsafe fn dict_get_string(dict: CFDictionaryRef, key: &str) -> Option<String> {
    unsafe {
        let value = dict_lookup(dict, key);
        if value.is_null() || CFGetTypeID(value) != CFStringGetTypeID() {
            return None;
        }
        let mut buf = vec![0i8; 256];
        let ok = CFStringGetCString(value as CFStringRef, buf.as_mut_ptr(), buf.len() as isize, kCFStringEncodingUTF8);
        if ok == 0 {
            return None;
        }
        Some(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
    }
}

#[derive(Debug, Clone, Default)]
pub struct BatteryReading {
    pub is_present: Option<bool>,
    pub is_charging: Option<bool>,
    pub power_source_state: Option<String>,
    pub percentage: Option<f64>,
    pub cycle_count: Option<i64>,
    /// `Max Capacity`/`Current Capacity` from this API are normalized to a
    /// 0-100 scale on macOS 12+ (Apple stopped exposing raw mAh here for
    /// privacy) — this is percentage-of-design, not the raw-capacity ratio
    /// vitals-gnome reads from Linux `/sys/class/power_supply`. Best-effort,
    /// `None` if the key isn't present on this OS version.
    pub health_percent: Option<f64>,
    /// mV — often absent on modern macOS (same privacy restriction), `None`
    /// rather than a fabricated value.
    pub voltage_mv: Option<i64>,
    /// mA, sign-normalized here (not trusted from the raw OS value):
    /// negative while discharging, positive while charging.
    pub power_rate_ma: Option<f64>,
    /// Raw per-tick minute estimate from IOKit — feed into
    /// [`TimeLeftEstimator`] rather than displaying directly, it jitters a
    /// lot tick to tick.
    pub raw_time_left_minutes: Option<f64>,
}

fn read_first_power_source_dict() -> Option<(CFTypeRef, CFDictionaryRef)> {
    unsafe {
        let blob = IOPSCopyPowerSourcesInfo();
        if blob.is_null() {
            return None;
        }
        let list = IOPSCopyPowerSourcesList(blob);
        if list.is_null() || CFArrayGetCount(list) == 0 {
            CFRelease(blob);
            return None;
        }
        let ps = CFArrayGetValueAtIndex(list, 0);
        let desc = IOPSGetPowerSourceDescription(blob, ps as CFTypeRef);
        // `list` is a separate +1-retained object from `blob`; release it,
        // but keep `blob` alive since `desc` borrows from it.
        CFRelease(list as CFTypeRef);
        if desc.is_null() {
            CFRelease(blob);
            return None;
        }
        Some((blob, desc))
    }
}

pub fn read_battery() -> BatteryReading {
    let Some((blob, desc)) = read_first_power_source_dict() else {
        return BatteryReading::default();
    };
    let reading = unsafe {
        let is_charging = dict_get_bool(desc, "Is Charging");
        let current = dict_get_i64(desc, "Current Capacity");
        let max = dict_get_i64(desc, "Max Capacity");
        let percentage = match (current, max) {
            (Some(c), Some(m)) if m > 0 => Some(c as f64 / m as f64 * 100.0),
            _ => None,
        };
        let raw_amperage = dict_get_i64(desc, "Amperage").or_else(|| dict_get_i64(desc, "InstantAmperage"));
        let power_rate_ma = match (raw_amperage, is_charging) {
            (Some(a), Some(charging)) => {
                let magnitude = (a as f64).abs();
                Some(if charging { magnitude } else { -magnitude })
            }
            _ => None,
        };

        let raw_time_left_minutes = if is_charging == Some(true) {
            dict_get_i64(desc, "Time to Full Charge")
        } else {
            dict_get_i64(desc, "Time to Empty")
        }
        .map(|m| m as f64);

        BatteryReading {
            is_present: dict_get_bool(desc, "Is Present"),
            is_charging,
            power_source_state: dict_get_string(desc, "Power Source State"),
            percentage,
            cycle_count: dict_get_i64(desc, "Cycle Count"),
            health_percent: max.map(|m| m as f64),
            voltage_mv: dict_get_i64(desc, "Voltage"),
            power_rate_ma,
            raw_time_left_minutes,
        }
    };
    unsafe {
        CFRelease(blob);
    }
    reading
}

/// 10-sample rolling average of estimated minutes remaining, reset whenever
/// charging/discharging state flips — matches vitals-gnome's smoothing
/// behavior so the panel number doesn't jump around every poll tick.
#[derive(Debug, Default)]
pub struct TimeLeftEstimator {
    samples: VecDeque<f64>,
    last_charging: Option<bool>,
}

impl TimeLeftEstimator {
    pub fn new() -> Self {
        Self::default()
    }

    /// `raw_minutes` should come from a fresh per-tick estimate; negative
    /// values (IOKit's "still calculating" sentinel) are ignored rather than
    /// polluting the average.
    pub fn push(&mut self, is_charging: bool, raw_minutes: Option<f64>) -> Option<f64> {
        if self.last_charging != Some(is_charging) {
            self.samples.clear();
            self.last_charging = Some(is_charging);
        }
        let raw = raw_minutes?;
        if raw < 0.0 {
            return None;
        }
        self.samples.push_back(raw);
        if self.samples.len() > 10 {
            self.samples.pop_front();
        }
        Some(self.samples.iter().sum::<f64>() / self.samples.len() as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resets_on_state_transition() {
        let mut est = TimeLeftEstimator::new();
        est.push(false, Some(60.0));
        est.push(false, Some(50.0));
        assert_eq!(est.push(false, Some(40.0)), Some(50.0));
        // flips to charging — buffer must reset, not blend with discharge samples
        assert_eq!(est.push(true, Some(20.0)), Some(20.0));
    }

    #[test]
    fn ignores_negative_sentinel() {
        let mut est = TimeLeftEstimator::new();
        assert_eq!(est.push(true, Some(-1.0)), None);
    }

    #[test]
    fn caps_at_ten_samples() {
        let mut est = TimeLeftEstimator::new();
        let mut last = None;
        for i in 1..=15 {
            last = est.push(true, Some(i as f64));
        }
        // last 10 of 1..=15 -> 6..=15, average = 10.5
        assert_eq!(last, Some(10.5));
    }
}
