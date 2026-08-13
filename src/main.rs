mod prefs;
mod sensors;
mod settings;
mod values;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSControlStateValueOff,
    NSControlStateValueOn, NSFont, NSFontWeightRegular, NSImage, NSMenu, NSMenuDelegate, NSMenuItem, NSStatusBar,
    NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSRunLoop, NSRunLoopCommonModes,
    NSString, NSTimer,
};
use std::cell::{OnceCell, RefCell};
use std::collections::{HashMap, HashSet};

use sensors::battery::{self, TimeLeftEstimator};
use sensors::cpu::{self, CpuSampler, CpuStaticInfo};
use sensors::network::NetworkAccumulator;
use sensors::public_ip::{self, PublicIpWatcher};
use sensors::smc::SmcConnection;
use sensors::thermal::{self, ThermalProbe};
use sensors::{
    disk, display_refresh, gpu, memory, network, system, wifi, SensorGroup, SensorHistory, SensorId, SensorReading,
};
use values::{FormatKind, FormatOptions, SizeUnit};

/// SF Symbol name per sensor/category — mirrors vitals-gnome's own
/// `icons/*-symbolic.svg` set (cpu/memory/temperature/voltage/gpu/network/
/// fan/system/storage/battery) conceptually, using macOS's built-in SF
/// Symbols instead of bundling SVG assets (same no-bundled-assets policy
/// already used for the flag emoji). `None` if no reasonable match exists;
/// the row just shows no icon rather than a wrong one.
fn sensor_icon_name(key: &str) -> Option<&'static str> {
    match key {
        "cpu_total" => Some("cpu"),
        "cpu_temp" => Some("thermometer"),
        "mem_usage" => Some("memorychip"),
        "swap_usage" => Some("arrow.triangle.2.circlepath"),
        "disk_usage" => Some("internaldrive"),
        "uptime" => Some("clock"),
        "net_up" => Some("arrow.up.circle"),
        "net_down" => Some("arrow.down.circle"),
        "wifi_quality" => Some("wifi"),
        "public_ip" => Some("globe"),
        "voltage_avg" => Some("bolt.fill"),
        "display_hz" => Some("display"),
        k if k.starts_with("fan_") => Some("fanblades.fill"),
        _ => None,
    }
}

fn category_icon_name(category_name: &str) -> Option<&'static str> {
    match category_name {
        "CPU" => Some("cpu"),
        "Memory" => Some("memorychip"),
        "System" => Some("gearshape"),
        "Storage" => Some("internaldrive"),
        "Network" => Some("network"),
        "Battery" => Some("battery.100"),
        "GPU" => Some("display"),
        _ => None,
    }
}

fn set_menu_item_icon(item: &NSMenuItem, symbol: Option<&'static str>) {
    let Some(symbol) = symbol else { return };
    if let Some(icon) = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(symbol), None)
    {
        icon.setTemplate(true);
        item.setImage(Some(&icon));
    }
}

/// Fixed display order for pinnable summary rows, per user preference —
/// overridden by alphabetical sort when `settings.alphabetize` is on.
/// Unlisted keys (e.g. `uptime`) sort just after Storage, their natural
/// spot, rather than vanishing from the ordering.
fn panel_order(key: &str) -> u8 {
    match key {
        "cpu_total" => 0,
        "cpu_temp" => 1,
        "mem_usage" => 2,
        "swap_usage" => 3,
        "disk_usage" => 4,
        "uptime" => 5,
        "net_up" => 6,
        "net_down" => 7,
        "wifi_quality" => 8,
        "public_ip" => 9,
        "fan_0" => 10,
        "fan_1" => 11,
        "fan_2" => 12,
        "fan_3" => 13,
        "voltage_avg" => 14,
        "display_hz" => 15,
        _ => 99,
    }
}

const ROOT_STORAGE_PATH: &str = "/";

/// Detail-row submenu categories, index-matched against `rebuild_menu`'s
/// `CAT_*` constants.
const CATEGORY_NAMES: [&str; 7] = ["CPU", "Memory", "System", "Storage", "Network", "Battery", "GPU"];

/// A menu-row-eligible sensor: clicking it toggles pin state, which mirrors
/// its formatted value onto the status bar panel text.
struct SensorDescriptor {
    key: &'static str,
    label: &'static str,
    raw_value: f64,
    formatted: String,
    /// False when this tick's read failed but a last-known value from a
    /// prior tick is being shown instead — the row stays visible but
    /// greyed (disabled) rather than vanishing mid-session.
    enabled: bool,
}

struct AppDelegateIvars {
    status_item: OnceCell<Retained<NSStatusItem>>,
    menu: OnceCell<Retained<NSMenu>>,
    static_info: OnceCell<CpuStaticInfo>,
    cpu_sampler: RefCell<CpuSampler>,
    history: RefCell<SensorHistory>,
    /// Mirrors `settings.hot_sensors` as a `HashSet` for O(1) membership
    /// checks; synced back into `settings` and persisted on every toggle.
    hot_sensors: RefCell<HashSet<String>>,
    /// Shared with the prefs window (`Rc::clone`d when it's opened) so both
    /// sides always see the same live settings without a callback closure.
    settings: std::rc::Rc<RefCell<settings::Settings>>,
    prefs_window: RefCell<Option<Retained<prefs::PrefsWindowController>>>,
    timer: RefCell<Option<Retained<NSTimer>>>,
    network_accumulator: RefCell<NetworkAccumulator>,
    battery_time_left: RefCell<TimeLeftEstimator>,
    public_ip_watcher: RefCell<PublicIpWatcher>,
    /// `None` if `AppleSMC` couldn't be opened at all (should not happen on
    /// real Mac hardware, but never assume) — thermal rows are simply
    /// omitted rather than panicking.
    smc: OnceCell<Option<SmcConnection>>,
    thermal_probe: OnceCell<ThermalProbe>,
    /// Last-known-good values for sensors whose backing key can disappear
    /// mid-session (SMC key drops out) — keeps the row visible-but-greyed
    /// instead of vanishing, per the interaction-model spec.
    last_known: RefCell<HashMap<String, f64>>,
    /// Pinned-row items, index-matched to each tick's `visible` list —
    /// reused via `setTitle`/`setState` in place instead of the whole menu
    /// being torn down and rebuilt every tick, so they refresh live
    /// without disturbing (or being disturbed by) an open category submenu.
    pinned_row_items: RefCell<Vec<Retained<NSMenuItem>>>,
    /// Per-category submenus — created once, reused forever. Their identity
    /// never changes, so an open submenu is never torn out from under the
    /// mouse (that was the cause of the original hover-blink bug).
    category_menus: OnceCell<Vec<Retained<NSMenu>>>,
    /// Per-category detail-row items, index-matched to each tick's info
    /// rows. Reused via `setTitle` in place when the row count for a
    /// category is unchanged (the common case — only the text changes);
    /// grown/shrunk only when the actual number of rows changes (e.g. a
    /// network interface appears/disappears).
    category_row_items: RefCell<Vec<Vec<Retained<NSMenuItem>>>>,
    /// True once the separator/header/action rows have been added — that
    /// structural section is built exactly once (see `rebuild_menu`), so
    /// an open submenu's parent header item is never destroyed.
    menu_structure_built: std::cell::Cell<bool>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = AppDelegateIvars]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let mtm = self.mtm();
            let bar = NSStatusBar::systemStatusBar();
            let item = bar.statusItemWithLength(NSVariableStatusItemLength);
            if let Some(button) = item.button(mtm) {
                button.setTitle(ns_string!("Pantau"));
                if !self.ivars().settings.borrow().hide_icons {
                    if let Some(icon) =
                        NSImage::imageWithSystemSymbolName_accessibilityDescription(ns_string!("gauge"), None)
                    {
                        icon.setTemplate(true); // auto-tints for light/dark menu bar
                        button.setImage(Some(&icon));
                        button.setImagePosition(objc2_app_kit::NSCellImagePosition::ImageLeft);
                    }
                }
                if self.ivars().settings.borrow().fixed_widths {
                    let font = NSFont::monospacedDigitSystemFontOfSize_weight(
                        NSFont::systemFontSize(),
                        unsafe { NSFontWeightRegular },
                    );
                    button.setFont(Some(&font));
                }
            }
            let menu = NSMenu::new(mtm);
            menu.setDelegate(Some(ProtocolObject::from_ref(self)));
            item.setMenu(Some(&menu));
            let _ = self.ivars().status_item.set(item);
            let _ = self.ivars().menu.set(menu);
            let _ = self.ivars().static_info.set(cpu::read_static_info());

            let smc = SmcConnection::open();
            if let Some(conn) = &smc {
                let _ = self.ivars().thermal_probe.set(ThermalProbe::discover(conn));
            }
            let _ = self.ivars().smc.set(smc);

            self.poll_tick_impl(true);
            self.schedule_timer();
        }
    }

    unsafe impl NSMenuDelegate for AppDelegate {
        #[unsafe(method(menuWillOpen:))]
        fn menu_will_open(&self, _menu: &NSMenu) {
            // Pinned rows and category submenu contents both update in
            // place every timer tick regardless of open/closed state (see
            // `rebuild_menu`), so opening just needs a fresh immediate poll.
            self.schedule_timer();
            self.poll_tick_impl(true);
        }
    }

    impl AppDelegate {
        #[unsafe(method(pollTick:))]
        fn poll_tick(&self, _timer: &NSTimer) {
            self.poll_tick_impl(false);
        }

        #[unsafe(method(pinToggle:))]
        fn pin_toggle(&self, sender: &NSMenuItem) {
            if let Some(obj) = sender.representedObject() {
                if let Ok(ns_key) = obj.downcast::<NSString>() {
                    let key = ns_key.to_string();
                    let mut hot = self.ivars().hot_sensors.borrow_mut();
                    if !hot.remove(&key) {
                        hot.insert(key);
                    }
                    drop(hot);
                    self.persist_hot_sensors();
                    self.poll_tick_impl(true);
                }
            }
        }

        #[unsafe(method(refreshTapped:))]
        fn refresh_tapped(&self, _sender: &NSMenuItem) {
            self.ivars().cpu_sampler.replace(CpuSampler::new());
            self.ivars().history.replace(SensorHistory::new());
            self.poll_tick_impl(true);
        }

        #[unsafe(method(systemMonitorTapped:))]
        fn system_monitor_tapped(&self, _sender: &NSMenuItem) {
            let _ = std::process::Command::new("open")
                .args(["-a", "Activity Monitor"])
                .spawn();
        }

        #[unsafe(method(preferencesTapped:))]
        fn preferences_tapped(&self, _sender: &NSMenuItem) {
            let mtm = self.mtm();
            let existing = self.ivars().prefs_window.borrow().clone();
            match existing {
                Some(controller) => controller.show(),
                None => {
                    let controller = prefs::PrefsWindowController::new(mtm, std::rc::Rc::clone(&self.ivars().settings));
                    controller.show();
                    *self.ivars().prefs_window.borrow_mut() = Some(controller);
                }
            }
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let settings = settings::Settings::load();
        let hot_sensors: HashSet<String> = settings.hot_sensors.iter().cloned().collect();
        let this = Self::alloc(mtm).set_ivars(AppDelegateIvars {
            status_item: OnceCell::new(),
            menu: OnceCell::new(),
            static_info: OnceCell::new(),
            cpu_sampler: RefCell::new(CpuSampler::new()),
            history: RefCell::new(SensorHistory::new()),
            hot_sensors: RefCell::new(hot_sensors),
            settings: std::rc::Rc::new(RefCell::new(settings)),
            prefs_window: RefCell::new(None),
            timer: RefCell::new(None),
            network_accumulator: RefCell::new(NetworkAccumulator::new()),
            battery_time_left: RefCell::new(TimeLeftEstimator::new()),
            public_ip_watcher: RefCell::new(PublicIpWatcher::new()),
            smc: OnceCell::new(),
            thermal_probe: OnceCell::new(),
            last_known: RefCell::new(HashMap::new()),
            pinned_row_items: RefCell::new(Vec::new()),
            category_menus: OnceCell::new(),
            category_row_items: RefCell::new((0..CATEGORY_NAMES.len()).map(|_| Vec::new()).collect()),
            menu_structure_built: std::cell::Cell::new(false),
        });
        unsafe { msg_send![super(this), init] }
    }

    /// Syncs `hot_sensors` back into `settings` and persists to disk —
    /// called after every pin toggle so pins survive a restart.
    fn persist_hot_sensors(&self) {
        let hot = self.ivars().hot_sensors.borrow();
        let mut settings = self.ivars().settings.borrow_mut();
        settings.hot_sensors = hot.iter().cloned().collect();
        let _ = settings.save();
    }

    fn schedule_timer(&self) {
        if let Some(old) = self.ivars().timer.borrow_mut().take() {
            old.invalidate();
        }
        let interval = self.ivars().settings.borrow().update_time.max(1) as f64;
        // `scheduledTimerWithTimeInterval...` only registers the timer in
        // NSDefaultRunLoopMode, which AppKit stops servicing while the run
        // loop is in event-tracking mode (a menu open, a window being
        // dragged, etc.) — the classic "menu-bar app stops updating unless
        // you interact with it" bug. Register in NSRunLoopCommonModes
        // instead so it keeps firing regardless of what mode the run loop
        // is in.
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                interval,
                self,
                sel!(pollTick:),
                None,
                true,
            )
        };
        unsafe {
            NSRunLoop::currentRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes);
        }
        *self.ivars().timer.borrow_mut() = Some(timer);
    }

    fn poll_tick_impl(&self, force: bool) {
        // The prefs window (if open) mutates `settings.hot_sensors` directly
        // (stash/restore on category toggle); resync our checkmark-lookup
        // HashSet from it so both stay consistent regardless of which side
        // last wrote.
        let synced_hot: HashSet<String> = self.ivars().settings.borrow().hot_sensors.iter().cloned().collect();
        *self.ivars().hot_sensors.borrow_mut() = synced_hot;

        let system_reading = system::read_system();
        let cpu_reading = self
            .ivars()
            .cpu_sampler
            .borrow_mut()
            .sample(system_reading.uptime_seconds);
        let memory_reading = memory::read_memory();
        let swap_reading = memory::read_swap();
        let disk_reading = disk::read_usage(ROOT_STORAGE_PATH);
        let network_readings = self.ivars().network_accumulator.borrow_mut().sample();
        let network_agg = network::aggregate(&network_readings);
        let wifi_reading = wifi::read_wifi();
        let battery_reading = battery::read_battery();
        let battery_time_left_minutes = battery_reading.is_charging.and_then(|charging| {
            self.ivars()
                .battery_time_left
                .borrow_mut()
                .push(charging, battery_reading.raw_time_left_minutes)
        });
        let thermal_reading = match (self.ivars().smc.get().and_then(|s| s.as_ref()), self.ivars().thermal_probe.get())
        {
            (Some(conn), Some(probe)) => Some(thermal::read(conn, probe)),
            _ => None,
        };
        let gpu_reading = gpu::read_utilization();
        let gpu_static = gpu::read_static_info();
        let display_hz = display_refresh::read_main_display_refresh_hz();

        let (public_ip_enabled, public_ip_interval, public_ip_provider, public_ip_show_flag) = {
            let settings = self.ivars().settings.borrow();
            let provider = match settings.network_public_ip_provider {
                settings::IpProvider::CoreCoding => public_ip::Provider::CoreCoding,
                settings::IpProvider::MyIp => public_ip::Provider::MyIp,
                settings::IpProvider::Ipify => public_ip::Provider::Ipify,
            };
            (
                settings.include_public_ip,
                settings.network_public_ip_interval,
                provider,
                settings.network_public_ip_show_flag,
            )
        };
        let public_ip_reading = self
            .ivars()
            .public_ip_watcher
            .borrow_mut()
            .poll(public_ip_enabled, public_ip_interval, public_ip_provider)
            .cloned();

        let opts = FormatOptions {
            higher_precision: self.ivars().settings.borrow().use_higher_precision,
        };

        // Read-with-fallback: use this tick's value if present, otherwise
        // fall back to the last-known-good value (row stays but greyed).
        // Returns None only if the sensor has never once read successfully.
        let mut last_known = self.ivars().last_known.borrow_mut();
        let mut resolve = |key: &str, fresh: Option<f64>| -> Option<(f64, bool)> {
            match fresh {
                Some(v) => {
                    last_known.insert(key.to_string(), v);
                    Some((v, true))
                }
                None => last_known.get(key).map(|&v| (v, false)),
            }
        };

        let mut descriptors: Vec<SensorDescriptor> = Vec::new();
        if let Some(cpu) = &cpu_reading {
            descriptors.push(SensorDescriptor {
                key: "cpu_total",
                label: "CPU",
                raw_value: cpu.aggregate_percent,
                formatted: format!("CPU: {}", FormatKind::Percent.format(cpu.aggregate_percent, opts)),
            enabled: true,
        });
        }
        if let Some(v) = memory_reading.usage_percent {
            descriptors.push(SensorDescriptor {
                key: "mem_usage",
                label: "Memory",
                raw_value: v,
                formatted: format!("Memory: {}", FormatKind::Percent.format(v, opts)),
            enabled: true,
        });
        }
        if let Some(v) = swap_reading.usage_percent {
            descriptors.push(SensorDescriptor {
                key: "swap_usage",
                label: "Swap",
                raw_value: v,
                formatted: format!("Swap: {}", FormatKind::Percent.format(v, opts)),
            enabled: true,
        });
        }
        if let Some(v) = disk_reading.used_percent {
            descriptors.push(SensorDescriptor {
                key: "disk_usage",
                label: "Storage",
                raw_value: v,
                formatted: format!("Storage: {}", FormatKind::Percent.format(v, opts)),
            enabled: true,
        });
        }
        if let Some(v) = system_reading.uptime_seconds {
            descriptors.push(SensorDescriptor {
                key: "uptime",
                label: "Uptime",
                raw_value: v,
                formatted: format!("Uptime: {}", FormatKind::Runtime.format(v, opts)),
            enabled: true,
        });
        }
        descriptors.push(SensorDescriptor {
            key: "net_down",
            label: "Network Down",
            raw_value: network_agg.rx_bytes_per_sec,
            formatted: format!(
                "Down: {}/s",
                FormatKind::Memory(SizeUnit::Decimal).format(network_agg.rx_bytes_per_sec, opts)
            ),
        enabled: true,
    });
        descriptors.push(SensorDescriptor {
            key: "net_up",
            label: "Network Up",
            raw_value: network_agg.tx_bytes_per_sec,
            formatted: format!(
                "Up: {}/s",
                FormatKind::Memory(SizeUnit::Decimal).format(network_agg.tx_bytes_per_sec, opts)
            ),
        enabled: true,
    });
        if let Some(v) = wifi_reading.link_quality_percent {
            descriptors.push(SensorDescriptor {
                key: "wifi_quality",
                label: "WiFi",
                raw_value: v,
                formatted: format!("WiFi: {}", FormatKind::Percent.format(v, opts)),
            enabled: true,
        });
        }

        if let Some((celsius, enabled)) = resolve(
            "cpu_temp",
            thermal_reading.as_ref().and_then(|t| t.cpu_temp_stats.map(|s| s.avg)),
        ) {
            let unit = self.ivars().settings.borrow().unit;
            let (display_temp, unit_symbol) = match unit {
                settings::TempUnit::Celsius => (celsius, "C"),
                settings::TempUnit::Fahrenheit => (celsius * 9.0 / 5.0 + 32.0, "F"),
            };
            descriptors.push(SensorDescriptor {
                key: "cpu_temp",
                label: "CPU Temp",
                // Dedupe stays in Celsius regardless of display unit — only
                // the formatted text converts.
                raw_value: celsius,
                formatted: format!("CPU Temp: {display_temp:.1}\u{00B0}{unit_symbol}"),
                enabled,
            });
        }
        const FAN_KEYS: [&str; 4] = ["fan_0", "fan_1", "fan_2", "fan_3"];
        if let Some(t) = &thermal_reading {
            for fan in &t.fans {
                let Some(&key) = FAN_KEYS.get(fan.index as usize) else {
                    continue; // more fans than we have static keys for — skip rather than leak
                };
                if let Some((rpm, enabled)) = resolve(key, fan.actual_rpm) {
                    descriptors.push(SensorDescriptor {
                        key,
                        label: "Fan",
                        raw_value: rpm,
                        formatted: format!("Fan {}: {:.0} RPM", fan.index, rpm),
                        enabled,
                    });
                }
            }
        }
        if let Some((volts, enabled)) = resolve("voltage_avg", thermal_reading.as_ref().and_then(|t| t.voltage_avg)) {
            descriptors.push(SensorDescriptor {
                key: "voltage_avg",
                label: "Voltage",
                raw_value: volts,
                formatted: format!("Voltage: {volts:.3} V"),
                enabled,
            });
        }
        if let Some(hz) = display_hz {
            descriptors.push(SensorDescriptor {
                key: "display_hz",
                label: "Display",
                raw_value: hz,
                formatted: format!("Display: {}", FormatKind::Hertz.format(hz, opts)),
                enabled: true,
            });
        }
        if let Some(reading) = &public_ip_reading {
            let flag = if public_ip_show_flag {
                reading
                    .country_code
                    .as_deref()
                    .and_then(public_ip::flag_emoji)
                    .map(|f| format!("{f} "))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let hash = reading.ip.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
            descriptors.push(SensorDescriptor {
                key: "public_ip",
                label: "Public IP",
                raw_value: hash as f64,
                formatted: format!("Public IP: {flag}{}", reading.ip),
                enabled: true,
            });
        }
        drop(last_known);
        descriptors.sort_by_key(|d| panel_order(d.key));

        let readings: Vec<SensorReading> = descriptors
            .iter()
            .map(|d| SensorReading {
                id: SensorId::new(SensorGroup::System, d.key),
                value: d.raw_value,
            })
            .collect();
        let changed = self.ivars().history.borrow_mut().update(&readings);
        if !changed && !force {
            return;
        }

        self.update_panel_text(&descriptors);
        // Always rebuild — every part of `rebuild_menu` updates items in
        // place rather than tearing the menu down, so values keep
        // refreshing live even while a submenu is open (see its comments).
        self.rebuild_menu(
            &descriptors,
            cpu_reading.as_ref(),
            &memory_reading,
            &swap_reading,
            &system_reading,
            &disk_reading,
            &network_readings,
            &wifi_reading,
            &battery_reading,
            battery_time_left_minutes,
            &gpu_reading,
            &gpu_static,
            opts,
        );
    }

    fn update_panel_text(&self, descriptors: &[SensorDescriptor]) {
        let mtm = self.mtm();
        let Some(status_item) = self.ivars().status_item.get() else {
            return;
        };
        let Some(button) = status_item.button(mtm) else {
            return;
        };
        let hot = self.ivars().hot_sensors.borrow();
        let pinned: Vec<&str> = descriptors
            .iter()
            .filter(|d| hot.contains(d.key))
            .map(|d| d.formatted.as_str())
            .collect();
        if pinned.is_empty() {
            // Default fallback when nothing is pinned — the gauge icon
            // (set once at launch) stays visible either way.
            button.setTitle(ns_string!("Pantau"));
        } else {
            // Hard character-length cap (not just an item-count cap) — an
            // unbounded-width button title makes macOS's menu-bar-overflow
            // logic evict this status item entirely (it vanishes from the
            // tray) once other menu bar items are already crowding the
            // available space. Individual formatted values vary a lot in
            // length, so capping by item count alone (previously 3) wasn't
            // enough on a crowded bar — a per-pin char budget bounds total
            // width regardless of how many/how long the pins are. All pins
            // still count as hot/toggled; only the on-screen text shrinks.
            const MAX_TITLE_CHARS: usize = 16;
            let mut text = String::new();
            let mut shown = 0;
            for value in &pinned {
                let candidate = if text.is_empty() { value.to_string() } else { format!("{text}  {value}") };
                if candidate.chars().count() > MAX_TITLE_CHARS && shown > 0 {
                    break;
                }
                text = candidate;
                shown += 1;
            }
            let text = if shown < pinned.len() {
                format!("{text} +{}", pinned.len() - shown)
            } else {
                text
            };
            // Right-pad to the same cap every tick — combined with the
            // width-padded FormatKind::Percent digits, this keeps the
            // status item's on-screen width constant instead of growing
            // and shrinking with every refresh as values change digit
            // count (e.g. "9%" -> "10%").
            button.setTitle(&NSString::from_str(&format!("{text:<MAX_TITLE_CHARS$}")));
        }
    }

    fn rebuild_menu(
        &self,
        descriptors: &[SensorDescriptor],
        cpu_reading: Option<&cpu::CpuReading>,
        memory_reading: &memory::MemoryReading,
        swap_reading: &memory::SwapReading,
        system_reading: &system::SystemReading,
        disk_reading: &disk::DiskReading,
        network_readings: &std::collections::HashMap<String, network::InterfaceReading>,
        wifi_reading: &wifi::WifiReading,
        battery_reading: &battery::BatteryReading,
        battery_time_left_minutes: Option<f64>,
        gpu_reading: &gpu::GpuReading,
        gpu_static: &gpu::GpuStaticInfo,
        opts: FormatOptions,
    ) {
        let Some(menu) = self.ivars().menu.get() else {
            return;
        };
        let mtm = self.mtm();

        let hot = self.ivars().hot_sensors.borrow().clone();

        let settings = self.ivars().settings.borrow();
        // Pinnable summary rows: hide-zero, except Fan — 0 RPM is a
        // valid idle reading, not noise, per the interaction-model
        // spec. Then alphabetize by label if enabled. Plain string
        // sort, not locale numeric-natural compare — good enough while
        // every label here is unique text, revisit if a numbered label
        // shows up.
        let mut visible: Vec<&SensorDescriptor> = descriptors
            .iter()
            .filter(|d| !settings.hide_zeros || d.raw_value != 0.0 || d.key.starts_with("fan_"))
            .collect();
        if settings.alphabetize {
            visible.sort_by(|a, b| a.label.cmp(b.label));
        }
        drop(settings);

        // Pinned rows are updated in place (`setTitle`/`setState` on
        // existing items, `insertItem_atIndex` only for genuinely new
        // rows) rather than `removeAllItems` + rebuild — this section runs
        // every tick unconditionally, live, even while a category submenu
        // is open below. Never touches items past its own row count, so
        // the separator/headers/actions built once further down are never
        // disturbed regardless of what's happening here.
        {
            let mut rows = self.ivars().pinned_row_items.borrow_mut();
            for (i, descriptor) in visible.iter().enumerate() {
                let state = if hot.contains(descriptor.key) { NSControlStateValueOn } else { NSControlStateValueOff };
                if let Some(item) = rows.get(i) {
                    item.setTitle(&NSString::from_str(&descriptor.formatted));
                    unsafe { item.setRepresentedObject(Some(&NSString::from_str(descriptor.key))) };
                    item.setState(state);
                    item.setEnabled(descriptor.enabled);
                } else {
                    let item = NSMenuItem::new(mtm);
                    item.setTitle(&NSString::from_str(&descriptor.formatted));
                    unsafe {
                        item.setTarget(Some(self));
                        item.setAction(Some(sel!(pinToggle:)));
                        item.setRepresentedObject(Some(&NSString::from_str(descriptor.key)));
                    }
                    item.setState(state);
                    item.setEnabled(descriptor.enabled);
                    set_menu_item_icon(&item, sensor_icon_name(descriptor.key));
                    menu.insertItem_atIndex(&item, i as isize);
                    rows.push(item);
                }
            }
            while rows.len() > visible.len() {
                if let Some(extra) = rows.pop() {
                    menu.removeItem(&extra);
                }
            }
        }

        // Detail rows grouped into per-category submenus (accordion) instead
        // of a flat list — cuts the top-level menu's scroll length. The
        // submenus themselves (and their row items) are created once and
        // reused forever via `category_menus`/`category_row_items` —
        // existing rows get `setTitle`'d in place rather than the whole
        // tree being torn down and rebuilt, which is what used to make an
        // open submenu visibly blink every refresh tick.
        let categories = self
            .ivars()
            .category_menus
            .get_or_init(|| (0..CATEGORY_NAMES.len()).map(|_| NSMenu::new(mtm)).collect());
        let mut row_counts = vec![0usize; CATEGORY_NAMES.len()];
        let mut add_info_row = |category: usize, title: String| {
            let mut rows = self.ivars().category_row_items.borrow_mut();
            let idx = row_counts[category];
            row_counts[category] += 1;
            if let Some(item) = rows[category].get(idx) {
                item.setTitle(&NSString::from_str(&title));
            } else {
                let item = NSMenuItem::new(mtm);
                item.setTitle(&NSString::from_str(&title));
                categories[category].addItem(&item);
                rows[category].push(item);
            }
        };

        const CAT_CPU: usize = 0;
        const CAT_MEMORY: usize = 1;
        const CAT_SYSTEM: usize = 2;
        const CAT_STORAGE: usize = 3;
        const CAT_NETWORK: usize = 4;
        const CAT_BATTERY: usize = 5;
        const CAT_GPU: usize = 6;

        if let Some(info) = self.ivars().static_info.get() {
            if let Some(brand) = &info.brand {
                add_info_row(CAT_CPU, brand.clone());
            }
            if let (Some(p), Some(l)) = (info.physical_cores, info.logical_cores) {
                add_info_row(CAT_CPU, format!("Cores: {p} physical / {l} logical"));
            }
        }
        if let Some(cpu) = cpu_reading {
            if let Some(stats) = cpu.group_stats {
                add_info_row(
                    CAT_CPU,
                    format!(
                        "  cores avg/min/max: {:.0}% / {:.0}% / {:.0}%",
                        stats.avg, stats.min, stats.max
                    ),
                );
            }
            for (i, pct) in cpu.per_core_percent.iter().enumerate() {
                add_info_row(CAT_CPU, format!("  Core {i}: {}", FormatKind::Percent.format(*pct, opts)));
            }
            if let Some(freq) = cpu::read_frequency_hz() {
                add_info_row(CAT_CPU, format!("CPU Frequency: {}", FormatKind::Hertz.format(freq as f64, opts)));
            }
            if let Some(pt) = cpu.process_time_seconds {
                add_info_row(CAT_CPU, format!("Process Time: {}", FormatKind::Runtime.format(pt, opts)));
            }
        }
        if let Some(v) = memory_reading.physical_bytes {
            add_info_row(
                CAT_MEMORY,
                format!("  Physical: {}", FormatKind::Memory(SizeUnit::Binary).format(v as f64, opts)),
            );
        }
        if let Some(v) = memory_reading.available_bytes {
            add_info_row(
                CAT_MEMORY,
                format!("  Available: {}", FormatKind::Memory(SizeUnit::Binary).format(v as f64, opts)),
            );
        }
        if let Some(v) = memory_reading.free_bytes {
            add_info_row(
                CAT_MEMORY,
                format!("  Free: {}", FormatKind::Memory(SizeUnit::Binary).format(v as f64, opts)),
            );
        }
        if let Some(v) = memory_reading.cached_approx_bytes {
            add_info_row(
                CAT_MEMORY,
                format!("  Cached (approx): {}", FormatKind::Memory(SizeUnit::Binary).format(v as f64, opts)),
            );
        }
        if let (Some(used), Some(total)) = (swap_reading.used_bytes, swap_reading.total_bytes) {
            add_info_row(
                CAT_MEMORY,
                format!(
                    "  Swap {} / {}",
                    FormatKind::Memory(SizeUnit::Binary).format(used as f64, opts),
                    FormatKind::Memory(SizeUnit::Binary).format(total as f64, opts)
                ),
            );
        }
        if let (Some(l1), Some(l5), Some(l15)) =
            (system_reading.load1, system_reading.load5, system_reading.load15)
        {
            add_info_row(
                CAT_SYSTEM,
                format!(
                    "Load: {} {} {}",
                    FormatKind::Load.format(l1, opts),
                    FormatKind::Load.format(l5, opts),
                    FormatKind::Load.format(l15, opts)
                ),
            );
        }
        if let Some(v) = system_reading.open_files {
            add_info_row(CAT_SYSTEM, format!("Open Files: {v}"));
        }
        if let (Some(threads), Some(procs)) = (system_reading.threads_total, system_reading.processes_total) {
            add_info_row(CAT_SYSTEM, format!("Threads: {threads} (Processes: {procs})"));
        }
        if let (Some(used), Some(total)) = (disk_reading.used_bytes, disk_reading.total_bytes) {
            add_info_row(
                CAT_STORAGE,
                format!(
                    "  Storage {} / {}",
                    FormatKind::Memory(SizeUnit::Decimal).format(used as f64, opts),
                    FormatKind::Memory(SizeUnit::Decimal).format(total as f64, opts)
                ),
            );
        }
        let mut interfaces: Vec<(&String, &network::InterfaceReading)> = network_readings.iter().collect();
        interfaces.sort_by(|a, b| a.0.cmp(b.0));
        for (name, reading) in interfaces {
            add_info_row(
                CAT_NETWORK,
                format!(
                    "  {name}: {}/s down, {}/s up",
                    FormatKind::Memory(SizeUnit::Decimal).format(reading.rx_bytes_per_sec, opts),
                    FormatKind::Memory(SizeUnit::Decimal).format(reading.tx_bytes_per_sec, opts)
                ),
            );
        }
        if let (Some(rssi), Some(noise)) = (wifi_reading.rssi_dbm, wifi_reading.noise_dbm) {
            add_info_row(CAT_NETWORK, format!("  WiFi {rssi} dBm (noise {noise} dBm)"));
        }
        if let Some(v) = battery_reading.cycle_count {
            add_info_row(CAT_BATTERY, format!("  Battery Cycle Count: {v}"));
        }
        if let Some(v) = battery_reading.health_percent {
            add_info_row(CAT_BATTERY, format!("  Battery Health: {}", FormatKind::Percent.format(v, opts)));
        }
        if let Some(v) = battery_reading.voltage_mv {
            add_info_row(CAT_BATTERY, format!("  Battery Voltage: {v} mV"));
        }
        if let Some(v) = battery_reading.power_rate_ma {
            add_info_row(CAT_BATTERY, format!("  Battery Rate: {v:+.0} mA"));
        }
        if let Some(v) = battery_time_left_minutes {
            add_info_row(
                CAT_BATTERY,
                format!("  Battery Time Left: {}", FormatKind::Runtime.format(v * 60.0, opts)),
            );
        }
        match (gpu_reading.utilization_percent, &gpu_static.model_name) {
            (None, None) => add_info_row(CAT_GPU, "  GPU: Unavailable (no public API on this Mac)".to_string()),
            _ => {
                if let Some(name) = &gpu_static.model_name {
                    add_info_row(CAT_GPU, format!("  GPU: {name}"));
                }
                if let Some(v) = gpu_reading.utilization_percent {
                    add_info_row(CAT_GPU, format!("  GPU Utilization: {}", FormatKind::Percent.format(v, opts)));
                }
            }
        }
        drop(add_info_row);

        // Trim stale rows left over from a shrunk category (e.g. a network
        // interface or fan that disappeared) — only touches the tail, the
        // reused rows above were already updated in place.
        {
            let mut rows = self.ivars().category_row_items.borrow_mut();
            for category in 0..CATEGORY_NAMES.len() {
                while rows[category].len() > row_counts[category] {
                    if let Some(extra) = rows[category].pop() {
                        categories[category].removeItem(&extra);
                    }
                }
            }
        }

        // Separator/headers/actions are structural and built exactly once
        // (on the very first tick, guaranteed to run before the menu can
        // possibly be open) — never rebuilt again, so an open category
        // submenu's parent header item is never destroyed out from under
        // it. Known simplification: a category with zero rows on that very
        // first tick never gets a header later even if it gains data (real
        // hardware has every sensor available from tick one, so this
        // doesn't bite in practice).
        if !self.ivars().menu_structure_built.get() {
            menu.addItem(&objc2_app_kit::NSMenuItem::separatorItem(mtm));

            for (name, submenu) in CATEGORY_NAMES.iter().zip(categories.iter()) {
                if submenu.numberOfItems() == 0 {
                    continue;
                }
                submenu.setDelegate(Some(ProtocolObject::from_ref(self)));
                let item = NSMenuItem::new(mtm);
                item.setTitle(&NSString::from_str(name));
                set_menu_item_icon(&item, category_icon_name(name));
                item.setSubmenu(Some(submenu));
                menu.addItem(&item);
            }

            menu.addItem(&objc2_app_kit::NSMenuItem::separatorItem(mtm));

            let action_row = |title: &str, action: objc2::runtime::Sel| {
                let item = NSMenuItem::new(mtm);
                item.setTitle(&NSString::from_str(title));
                unsafe {
                    item.setTarget(Some(self));
                    item.setAction(Some(action));
                }
                menu.addItem(&item);
            };
            action_row("Refresh", sel!(refreshTapped:));
            action_row("System Monitor", sel!(systemMonitorTapped:));
            action_row("Preferences...", sel!(preferencesTapped:));

            self.ivars().menu_structure_built.set(true);
        }
    }
}

fn main() {
    let mtm = MainThreadMarker::new().expect("must run on main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let delegate = AppDelegate::new(mtm);
    app.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(&*delegate)));

    app.run();
}
