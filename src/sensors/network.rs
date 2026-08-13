use std::collections::HashMap;
use std::ffi::CStr;
use std::time::Instant;

const AF_LINK: libc::c_int = 18;
const LOOPBACK_PREFIX: &str = "lo";

#[derive(Debug, Clone, Copy, Default)]
struct RawCounters {
    rx_bytes: u64,
    tx_bytes: u64,
}

/// One `getifaddrs` pass, `AF_LINK` entries only, loopback excluded.
fn read_raw_counters() -> HashMap<String, RawCounters> {
    let mut out = HashMap::new();
    unsafe {
        let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut head) != 0 {
            return out;
        }
        let mut cursor = head;
        while !cursor.is_null() {
            let entry = &*cursor;
            cursor = entry.ifa_next;

            if entry.ifa_name.is_null() || entry.ifa_addr.is_null() || entry.ifa_data.is_null() {
                continue;
            }
            if (*entry.ifa_addr).sa_family as libc::c_int != AF_LINK {
                continue;
            }
            let name = CStr::from_ptr(entry.ifa_name).to_string_lossy().into_owned();
            if name.starts_with(LOOPBACK_PREFIX) {
                continue;
            }
            // macOS populates AF_LINK ifa_data as `struct if_data64`.
            let data = &*(entry.ifa_data as *const libc::if_data64);
            let counters = out.entry(name).or_insert(RawCounters::default());
            counters.rx_bytes += data.ifi_ibytes;
            counters.tx_bytes += data.ifi_obytes;
        }
        libc::freeifaddrs(head);
    }
    out
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InterfaceReading {
    pub rx_bytes_total: u64,
    pub tx_bytes_total: u64,
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    /// Bytes since this process started watching — resets on app restart,
    /// unlike `rx_bytes_total`/`tx_bytes_total` which are cumulative since
    /// the interface came up (kernel-tracked, closest available analog to
    /// "boot total").
    pub rx_bytes_session: u64,
    pub tx_bytes_session: u64,
}

#[derive(Debug, Clone, Copy)]
struct Baseline {
    rx: u64,
    tx: u64,
}

/// Tracks per-interface deltas for rate calculation, plus a session-start
/// baseline captured on first sample so callers can report "since app
/// launch" totals distinct from the kernel's cumulative-since-link-up counters.
#[derive(Debug, Default)]
pub struct NetworkAccumulator {
    previous: HashMap<String, (RawCounters, Instant)>,
    session_baseline: HashMap<String, Baseline>,
}

impl NetworkAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sample(&mut self) -> HashMap<String, InterfaceReading> {
        let now = Instant::now();
        let raw = read_raw_counters();
        let mut out = HashMap::with_capacity(raw.len());

        for (name, counters) in &raw {
            self.session_baseline.entry(name.clone()).or_insert(Baseline {
                rx: counters.rx_bytes,
                tx: counters.tx_bytes,
            });
            let baseline = self.session_baseline[name];

            let (rx_rate, tx_rate) = match self.previous.get(name) {
                Some((prev, prev_time)) => {
                    let elapsed = now.duration_since(*prev_time).as_secs_f64();
                    if elapsed > 0.0 {
                        (
                            (counters.rx_bytes.saturating_sub(prev.rx_bytes)) as f64 / elapsed,
                            (counters.tx_bytes.saturating_sub(prev.tx_bytes)) as f64 / elapsed,
                        )
                    } else {
                        (0.0, 0.0)
                    }
                }
                None => (0.0, 0.0),
            };

            out.insert(
                name.clone(),
                InterfaceReading {
                    rx_bytes_total: counters.rx_bytes,
                    tx_bytes_total: counters.tx_bytes,
                    rx_bytes_per_sec: rx_rate,
                    tx_bytes_per_sec: tx_rate,
                    rx_bytes_session: counters.rx_bytes.saturating_sub(baseline.rx),
                    tx_bytes_session: counters.tx_bytes.saturating_sub(baseline.tx),
                },
            );
            self.previous.insert(name.clone(), (*counters, now));
        }

        out
    }
}

/// Sum of all non-loopback interfaces' current rates/totals — the "device
/// aggregate" view.
pub fn aggregate(readings: &HashMap<String, InterfaceReading>) -> InterfaceReading {
    let mut agg = InterfaceReading::default();
    for r in readings.values() {
        agg.rx_bytes_total += r.rx_bytes_total;
        agg.tx_bytes_total += r.tx_bytes_total;
        agg.rx_bytes_per_sec += r.rx_bytes_per_sec;
        agg.tx_bytes_per_sec += r.tx_bytes_per_sec;
        agg.rx_bytes_session += r.rx_bytes_session;
        agg.tx_bytes_session += r.tx_bytes_session;
    }
    agg
}
