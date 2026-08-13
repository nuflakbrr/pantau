//! Public IP fetch — same 3 providers and endpoints as vitals-gnome's
//! `sensors.js` `_refreshIPAddress` (verified against that source, not
//! guessed): Core Coding, MyIP.com, ipify. Runs on a background thread per
//! the plan's async model (HTTP is the one genuinely slow operation in this
//! app; everything else is a fast local syscall on the main thread) —
//! result comes back over a channel, polled non-blockingly from the timer tick.
use serde::Deserialize;
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    CoreCoding,
    MyIp,
    Ipify,
}

#[derive(Debug, Clone)]
pub struct PublicIpReading {
    pub ip: String,
    /// Lowercase 2-letter ISO code, `None` if the provider doesn't return
    /// one (ipify) or reports it as unknown (MyIP.com's `"xx"` sentinel).
    pub country_code: Option<String>,
}

#[derive(Deserialize)]
struct CoreCodingResponse {
    #[serde(rename = "IPv4")]
    ipv4: String,
    #[serde(rename = "countryCode")]
    country_code: Option<String>,
}

#[derive(Deserialize)]
struct MyIpResponse {
    ip: String,
    cc: Option<String>,
}

#[derive(Deserialize)]
struct IpifyResponse {
    ip: String,
}

/// Core Coding's endpoint gates access server-side to `{"info":"For Vitals
/// Gnome extension use only."}` for any client that isn't the actual GNOME
/// extension (confirmed via curl during development — not a guess, and not
/// something worth spoofing headers to work around). The JSON shape mismatch
/// makes `into_json` fail, so this returns `None` — same graceful-degradation
/// contract as every other unavailable sensor in this codebase, no special
/// case needed. Real-world effect: this provider is currently unusable from
/// pantau-app; `MyIp`/`Ipify` are unaffected and both verified working.
fn fetch_core_coding() -> Option<PublicIpReading> {
    let resp: CoreCodingResponse = ureq::get("https://ipv4.corecoding.com").call().ok()?.into_json().ok()?;
    Some(PublicIpReading {
        ip: resp.ipv4,
        country_code: resp.country_code.map(|c| c.to_lowercase()),
    })
}

fn fetch_myip() -> Option<PublicIpReading> {
    let resp: MyIpResponse = ureq::get("https://api.myip.com").call().ok()?.into_json().ok()?;
    let country_code = resp.cc.map(|c| c.to_lowercase()).filter(|c| c != "xx");
    Some(PublicIpReading { ip: resp.ip, country_code })
}

fn fetch_ipify() -> Option<PublicIpReading> {
    let resp: IpifyResponse = ureq::get("https://api.ipify.org?format=json").call().ok()?.into_json().ok()?;
    Some(PublicIpReading { ip: resp.ip, country_code: None })
}

fn fetch(provider: Provider) -> Option<PublicIpReading> {
    match provider {
        Provider::CoreCoding => fetch_core_coding(),
        Provider::MyIp => fetch_myip(),
        Provider::Ipify => fetch_ipify(),
    }
}

/// Regional-indicator-symbol emoji flag from a 2-letter ISO country code —
/// no bundled flag assets (license-clean default per the plan), works for
/// any code without needing a lookup table.
pub fn flag_emoji(country_code: &str) -> Option<String> {
    let upper: Vec<char> = country_code.to_uppercase().chars().collect();
    if upper.len() != 2 || !upper.iter().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    upper
        .iter()
        .map(|&c| char::from_u32(0x1F1E6 + (c as u32 - 'A' as u32)))
        .collect::<Option<String>>()
}

/// Background-thread fetch scheduler: spawns a new lookup when the interval
/// has elapsed and no fetch is already in flight, drains a completed result
/// non-blockingly. Never blocks the caller (the main thread's poll tick).
#[derive(Default)]
pub struct PublicIpWatcher {
    receiver: Option<mpsc::Receiver<PublicIpReading>>,
    last_fetch: Option<Instant>,
    latest: Option<PublicIpReading>,
}

impl PublicIpWatcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn poll(&mut self, enabled: bool, interval_seconds: u32, provider: Provider) -> Option<&PublicIpReading> {
        if !enabled {
            return None;
        }
        if let Some(rx) = &self.receiver {
            if let Ok(reading) = rx.try_recv() {
                self.latest = Some(reading);
                self.receiver = None;
            }
        }
        let due = self
            .last_fetch
            .map(|t| t.elapsed() >= Duration::from_secs(interval_seconds.max(1) as u64))
            .unwrap_or(true);
        if due && self.receiver.is_none() {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                if let Some(reading) = fetch(provider) {
                    let _ = tx.send(reading);
                }
            });
            self.receiver = Some(rx);
            self.last_fetch = Some(Instant::now());
        }
        self.latest.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_emoji_from_valid_code() {
        assert_eq!(flag_emoji("us"), Some("\u{1F1FA}\u{1F1F8}".to_string()));
        assert_eq!(flag_emoji("ID"), Some("\u{1F1EE}\u{1F1E9}".to_string()));
    }

    #[test]
    fn flag_emoji_rejects_invalid_input() {
        assert_eq!(flag_emoji("xx1"), None);
        assert_eq!(flag_emoji("u"), None);
        assert_eq!(flag_emoji(""), None);
    }

    // Real-network smoke test, not run by default (`cargo test -- --ignored`).
    // CoreCoding is excluded from the hard assertion — its endpoint gates
    // access to the actual GNOME extension only (see `fetch_core_coding`'s
    // doc comment), confirmed via curl, not a bug in this code.
    #[test]
    #[ignore]
    fn fetches_from_all_three_providers() {
        for provider in [Provider::CoreCoding, Provider::MyIp, Provider::Ipify] {
            let reading = fetch(provider);
            println!("{provider:?} -> {reading:?}");
            if provider != Provider::CoreCoding {
                assert!(reading.is_some(), "{provider:?} should return a reading");
            }
        }
    }
}
