//! Preferences window. Deliberate simplifications vs. the original plan,
//! both noted rather than silent:
//! - Sidebar nav uses `NSSegmentedControl` instead of `NSSplitViewController`
//!   + `NSTableView` — same category-to-detail-pane behavior, far less
//!   ceremony (no `NSTableViewDataSource`/`Delegate` conformance needed).
//! - All 5 panes live in this one file as builder functions instead of 5
//!   separate `NSViewController` subclasses — Rust modules don't need
//!   Swift/ObjC's one-controller-per-file convention, and a real
//!   `NSViewController` per pane would 5x the `define_class!` boilerplate
//!   for no behavioral gain here (panes are rebuilt fresh on every segment
//!   switch, so no per-controller state needs to persist between switches).
//! - No Colors pane: the severity-dot renderer it used to configure
//!   (`percent_dot`/`temp_dot`) was removed from the menu display, so a
//!   threshold editor for it would edit dead settings.
use objc2::rc::Retained;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSButtonType, NSImage, NSImageView, NSPopUpButton, NSSegmentedControl, NSTextField,
    NSView, NSWindow, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL,
};
use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::settings::{IpProvider, Settings, SizeUnit, SpeedFormat, TempUnit};

const PANE_TITLES: [&str; 5] = ["General", "Sensors", "Units", "Network", "About"];

const TAG_HIGHER_PRECISION: isize = 1;
const TAG_HIDE_ZEROS: isize = 3;
const TAG_FIXED_WIDTHS: isize = 4;

const TAG_SHOW_TEMPERATURE: isize = 10;
const TAG_SHOW_VOLTAGE: isize = 11;
const TAG_SHOW_FAN: isize = 12;
const TAG_SHOW_MEMORY: isize = 13;
const TAG_SHOW_PROCESSOR: isize = 14;
const TAG_SHOW_SYSTEM: isize = 15;
const TAG_SHOW_STORAGE: isize = 16;
const TAG_SHOW_NETWORK: isize = 17;
const TAG_SHOW_BATTERY: isize = 18;
const TAG_SHOW_GPU: isize = 19;

const TAG_INCLUDE_PUBLIC_IP: isize = 20;
const TAG_SHOW_FLAG: isize = 21;

const TAG_UNIT_POPUP: isize = 100;
const TAG_MEMORY_MEASUREMENT_POPUP: isize = 101;
const TAG_STORAGE_MEASUREMENT_POPUP: isize = 102;
const TAG_SPEED_FORMAT_POPUP: isize = 103;
const TAG_IP_PROVIDER_POPUP: isize = 104;

const TAG_GITHUB_LINK: isize = 200;
const TAG_ISSUES_LINK: isize = 201;
const TAG_AUTHOR_LINK: isize = 202;

/// Sensor-key prefix -> the `Settings` category field it belongs to, for
/// hot-sensor stash/restore when a category gets toggled off/on.
fn category_for_key(key: &str) -> Option<&'static str> {
    match key {
        "cpu_total" => Some("processor"),
        "mem_usage" | "swap_usage" => Some("memory"),
        "load1" | "uptime" | "display_hz" => Some("system"),
        "disk_usage" => Some("storage"),
        "net_down" | "net_up" | "wifi_quality" => Some("network"),
        "battery_percentage" => Some("battery"),
        "cpu_temp" => Some("temperature"),
        "voltage_avg" => Some("voltage"),
        k if k.starts_with("fan_") => Some("fan"),
        _ => None,
    }
}

pub struct PrefsWindowIvars {
    window: OnceCell<Retained<NSWindow>>,
    segmented: OnceCell<Retained<NSSegmentedControl>>,
    content_container: OnceCell<Retained<NSView>>,
    /// Shared with `AppDelegate` — mutations here are immediately visible
    /// there too, no callback plumbing needed.
    settings: Rc<RefCell<Settings>>,
    /// category key -> stashed hot_sensor keys, restored if the category is
    /// re-enabled before the window closes.
    stashed: RefCell<HashMap<&'static str, Vec<String>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = PrefsWindowIvars]
    pub struct PrefsWindowController;

    unsafe impl NSObjectProtocol for PrefsWindowController {}

    impl PrefsWindowController {
        #[unsafe(method(segmentChanged:))]
        fn segment_changed(&self, _sender: &NSSegmentedControl) {
            self.rebuild_pane();
        }

        #[unsafe(method(toggleField:))]
        fn toggle_field(&self, sender: &NSButton) {
            self.handle_toggle(sender.tag());
        }

        #[unsafe(method(popupChanged:))]
        fn popup_changed(&self, sender: &NSPopUpButton) {
            self.handle_popup(sender.tag(), sender.indexOfSelectedItem());
        }

        #[unsafe(method(openLink:))]
        fn open_link(&self, sender: &NSButton) {
            let url_str = match sender.tag() {
                TAG_GITHUB_LINK => "https://github.com/nuflakbrr/pantau",
                TAG_ISSUES_LINK => "https://github.com/nuflakbrr/pantau/issues/new",
                TAG_AUTHOR_LINK => "https://github.com/nuflakbrr",
                _ => return,
            };
            if let Some(url) = NSURL::URLWithString(&NSString::from_str(url_str)) {
                NSWorkspace::sharedWorkspace().openURL(&url);
            }
        }
    }
);

impl PrefsWindowController {
    pub fn new(mtm: MainThreadMarker, settings: Rc<RefCell<Settings>>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PrefsWindowIvars {
            window: OnceCell::new(),
            segmented: OnceCell::new(),
            content_container: OnceCell::new(),
            settings,
            stashed: RefCell::new(HashMap::new()),
        });
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        this.build_window(mtm);
        this
    }

    pub fn show(&self) {
        if let Some(window) = self.ivars().window.get() {
            window.center();
            window.makeKeyAndOrderFront(None);
        }
    }

    fn build_window(&self, mtm: MainThreadMarker) {
        let frame = NSRect { origin: NSPoint { x: 0.0, y: 0.0 }, size: NSSize { width: 480.0, height: 420.0 } };
        let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                frame,
                style,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        window.setTitle(ns_string!("Preferences"));
        unsafe { window.setReleasedWhenClosed(false) };

        let content = NSView::new(mtm);
        content.setFrame(frame);

        let segmented = NSSegmentedControl::new(mtm);
        segmented.setSegmentCount(PANE_TITLES.len() as isize);
        for (i, title) in PANE_TITLES.iter().enumerate() {
            segmented.setLabel_forSegment(&NSString::from_str(title), i as isize);
        }
        unsafe {
            segmented.setTarget(Some(self));
            segmented.setAction(Some(sel!(segmentChanged:)));
        }
        segmented.setSelectedSegment(0);
        segmented.setFrame(NSRect {
            origin: NSPoint { x: 16.0, y: 380.0 },
            size: NSSize { width: 448.0, height: 24.0 },
        });

        let pane_container = NSView::new(mtm);
        pane_container.setFrame(NSRect {
            origin: NSPoint { x: 16.0, y: 16.0 },
            size: NSSize { width: 448.0, height: 350.0 },
        });

        content.addSubview(&segmented);
        content.addSubview(&pane_container);
        window.setContentView(Some(&content));

        let _ = self.ivars().window.set(window);
        let _ = self.ivars().segmented.set(segmented);
        let _ = self.ivars().content_container.set(pane_container);

        self.rebuild_pane();
    }

    fn rebuild_pane(&self) {
        let mtm = self.mtm();
        let Some(container) = self.ivars().content_container.get() else {
            return;
        };
        for view in container.subviews().iter() {
            view.removeFromSuperview();
        }
        let index = self.ivars().segmented.get().map(|s| s.selectedSegment()).unwrap_or(0);
        let settings = self.ivars().settings.borrow();
        let view = match index {
            0 => build_general_pane(mtm, self, &settings),
            1 => build_sensors_pane(mtm, self, &settings),
            2 => build_units_pane(mtm, self, &settings),
            3 => build_network_pane(mtm, self, &settings),
            _ => build_about_pane(mtm, self),
        };
        drop(settings);
        container.addSubview(&view);
    }

    fn handle_toggle(&self, tag: isize) {
        let mut settings = self.ivars().settings.borrow_mut();
        let field = match tag {
            TAG_HIGHER_PRECISION => &mut settings.use_higher_precision,
            TAG_HIDE_ZEROS => &mut settings.hide_zeros,
            TAG_FIXED_WIDTHS => &mut settings.fixed_widths,
            TAG_INCLUDE_PUBLIC_IP => &mut settings.include_public_ip,
            TAG_SHOW_FLAG => &mut settings.network_public_ip_show_flag,
            _ => {
                drop(settings);
                self.handle_category_toggle(tag);
                return;
            }
        };
        *field = !*field;
        drop(settings);
        self.persist();
        self.rebuild_pane();
    }

    fn handle_category_toggle(&self, tag: isize) {
        let category_key = match tag {
            TAG_SHOW_TEMPERATURE => "temperature",
            TAG_SHOW_VOLTAGE => "voltage",
            TAG_SHOW_FAN => "fan",
            TAG_SHOW_MEMORY => "memory",
            TAG_SHOW_PROCESSOR => "processor",
            TAG_SHOW_SYSTEM => "system",
            TAG_SHOW_STORAGE => "storage",
            TAG_SHOW_NETWORK => "network",
            TAG_SHOW_BATTERY => "battery",
            TAG_SHOW_GPU => "gpu",
            _ => return,
        };
        let now_enabled = {
            let mut settings = self.ivars().settings.borrow_mut();
            let field = match category_key {
                "temperature" => &mut settings.show_temperature,
                "voltage" => &mut settings.show_voltage,
                "fan" => &mut settings.show_fan,
                "memory" => &mut settings.show_memory,
                "processor" => &mut settings.show_processor,
                "system" => &mut settings.show_system,
                "storage" => &mut settings.show_storage,
                "network" => &mut settings.show_network,
                "battery" => &mut settings.show_battery,
                "gpu" => &mut settings.show_gpu,
                _ => return,
            };
            *field = !*field;
            *field
        };

        // Session-scoped hot-sensor stash/restore: turning a category off
        // hides its pinned rows but remembers them; turning it back on
        // before the window closes restores the pins rather than losing them.
        let mut settings = self.ivars().settings.borrow_mut();
        let mut stashed = self.ivars().stashed.borrow_mut();
        if now_enabled {
            if let Some(restored) = stashed.remove(category_key) {
                for key in restored {
                    if !settings.hot_sensors.contains(&key) {
                        settings.hot_sensors.push(key);
                    }
                }
            }
        } else {
            let (moved, kept): (Vec<String>, Vec<String>) = settings
                .hot_sensors
                .drain(..)
                .partition(|k| category_for_key(k) == Some(category_key));
            settings.hot_sensors = kept;
            if !moved.is_empty() {
                stashed.insert(category_key, moved);
            }
        }
        drop(stashed);
        drop(settings);
        self.persist();
        self.rebuild_pane();
    }

    fn handle_popup(&self, tag: isize, index: isize) {
        let mut settings = self.ivars().settings.borrow_mut();
        match tag {
            TAG_UNIT_POPUP => settings.unit = if index == 0 { TempUnit::Celsius } else { TempUnit::Fahrenheit },
            TAG_MEMORY_MEASUREMENT_POPUP => {
                settings.memory_measurement = if index == 0 { SizeUnit::Binary } else { SizeUnit::Decimal }
            }
            TAG_STORAGE_MEASUREMENT_POPUP => {
                settings.storage_measurement = if index == 0 { SizeUnit::Binary } else { SizeUnit::Decimal }
            }
            TAG_SPEED_FORMAT_POPUP => {
                settings.network_speed_format = if index == 0 { SpeedFormat::Bits } else { SpeedFormat::Bytes }
            }
            TAG_IP_PROVIDER_POPUP => {
                settings.network_public_ip_provider = match index {
                    0 => IpProvider::CoreCoding,
                    1 => IpProvider::MyIp,
                    _ => IpProvider::Ipify,
                };
            }
            _ => return,
        }
        drop(settings);
        self.persist();
        self.rebuild_pane();
    }

    fn persist(&self) {
        let _ = self.ivars().settings.borrow().save();
    }
}

fn label(mtm: MainThreadMarker, text: &str, y: f64) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    field.setFrame(NSRect { origin: NSPoint { x: 0.0, y }, size: NSSize { width: 420.0, height: 20.0 } });
    field
}

fn checkbox(
    mtm: MainThreadMarker,
    controller: &PrefsWindowController,
    title: &str,
    checked: bool,
    tag: isize,
    y: f64,
) -> Retained<NSButton> {
    let button = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(title),
            Some(controller),
            Some(sel!(toggleField:)),
            mtm,
        )
    };
    button.setButtonType(NSButtonType::Switch);
    button.setState(if checked {
        objc2_app_kit::NSControlStateValueOn
    } else {
        objc2_app_kit::NSControlStateValueOff
    });
    button.setTag(tag);
    button.setFrame(NSRect { origin: NSPoint { x: 0.0, y }, size: NSSize { width: 420.0, height: 20.0 } });
    button
}

fn popup(
    mtm: MainThreadMarker,
    controller: &PrefsWindowController,
    items: &[&str],
    selected: usize,
    tag: isize,
    enabled: bool,
    y: f64,
) -> Retained<NSPopUpButton> {
    let control = NSPopUpButton::new(mtm);
    for item in items {
        control.addItemWithTitle(&NSString::from_str(item));
    }
    control.selectItemAtIndex(selected as isize);
    unsafe {
        control.setTarget(Some(controller));
        control.setAction(Some(sel!(popupChanged:)));
    }
    control.setTag(tag);
    control.setEnabled(enabled);
    control.setFrame(NSRect { origin: NSPoint { x: 180.0, y }, size: NSSize { width: 200.0, height: 24.0 } });
    control
}

fn link_button(
    mtm: MainThreadMarker,
    controller: &PrefsWindowController,
    title: &str,
    tag: isize,
    x: f64,
    y: f64,
    width: f64,
) -> Retained<NSButton> {
    let button = unsafe {
        NSButton::buttonWithTitle_target_action(&NSString::from_str(title), Some(controller), Some(sel!(openLink:)), mtm)
    };
    button.setBezelStyle(objc2_app_kit::NSBezelStyle::Push);
    button.setTag(tag);
    button.setFrame(NSRect { origin: NSPoint { x, y }, size: NSSize { width, height: 28.0 } });
    button
}

/// A borderless, text-sized link — for an inline credit line, unlike
/// `link_button`'s full bezeled button meant for a standalone row.
fn inline_link(mtm: MainThreadMarker, controller: &PrefsWindowController, title: &str, tag: isize, y: f64) -> Retained<NSButton> {
    let button = unsafe {
        NSButton::buttonWithTitle_target_action(&NSString::from_str(title), Some(controller), Some(sel!(openLink:)), mtm)
    };
    button.setBordered(false);
    button.setAlignment(objc2_app_kit::NSTextAlignment::Left);
    button.setTag(tag);
    button.setFrame(NSRect { origin: NSPoint { x: 0.0, y }, size: NSSize { width: 300.0, height: 20.0 } });
    button
}

fn container_view(mtm: MainThreadMarker) -> Retained<NSView> {
    let view = NSView::new(mtm);
    view.setFrame(NSRect {
        origin: NSPoint { x: 0.0, y: 0.0 },
        size: NSSize { width: 448.0, height: 350.0 },
    });
    view
}

fn build_general_pane(mtm: MainThreadMarker, c: &PrefsWindowController, s: &Settings) -> Retained<NSView> {
    let view = container_view(mtm);
    view.addSubview(&checkbox(mtm, c, "Use higher precision", s.use_higher_precision, TAG_HIGHER_PRECISION, 310.0));
    view.addSubview(&checkbox(mtm, c, "Hide zero-value sensors", s.hide_zeros, TAG_HIDE_ZEROS, 280.0));
    view.addSubview(&checkbox(mtm, c, "Fixed-width panel labels", s.fixed_widths, TAG_FIXED_WIDTHS, 250.0));
    view
}

fn build_sensors_pane(mtm: MainThreadMarker, c: &PrefsWindowController, s: &Settings) -> Retained<NSView> {
    let view = container_view(mtm);
    let rows: [(&str, bool, isize); 10] = [
        ("Temperature", s.show_temperature, TAG_SHOW_TEMPERATURE),
        ("Voltage", s.show_voltage, TAG_SHOW_VOLTAGE),
        ("Fan", s.show_fan, TAG_SHOW_FAN),
        ("Memory", s.show_memory, TAG_SHOW_MEMORY),
        ("Processor", s.show_processor, TAG_SHOW_PROCESSOR),
        ("System", s.show_system, TAG_SHOW_SYSTEM),
        ("Storage", s.show_storage, TAG_SHOW_STORAGE),
        ("Network", s.show_network, TAG_SHOW_NETWORK),
        ("Battery", s.show_battery, TAG_SHOW_BATTERY),
        ("GPU", s.show_gpu, TAG_SHOW_GPU),
    ];
    for (i, (title, checked, tag)) in rows.iter().enumerate() {
        let y = 310.0 - (i as f64 * 30.0);
        view.addSubview(&checkbox(mtm, c, title, *checked, *tag, y));
    }
    view
}

fn build_units_pane(mtm: MainThreadMarker, c: &PrefsWindowController, s: &Settings) -> Retained<NSView> {
    let view = container_view(mtm);
    view.addSubview(&label(mtm, "Temperature unit", 310.0));
    view.addSubview(&popup(
        mtm,
        c,
        &["Celsius", "Fahrenheit"],
        if s.unit == TempUnit::Celsius { 0 } else { 1 },
        TAG_UNIT_POPUP,
        s.show_temperature,
        310.0,
    ));
    view.addSubview(&label(mtm, "Memory measurement", 270.0));
    view.addSubview(&popup(
        mtm,
        c,
        &["Binary (GiB)", "Decimal (GB)"],
        if s.memory_measurement == SizeUnit::Binary { 0 } else { 1 },
        TAG_MEMORY_MEASUREMENT_POPUP,
        s.show_memory,
        270.0,
    ));
    view.addSubview(&label(mtm, "Storage measurement", 230.0));
    view.addSubview(&popup(
        mtm,
        c,
        &["Binary (GiB)", "Decimal (GB)"],
        if s.storage_measurement == SizeUnit::Binary { 0 } else { 1 },
        TAG_STORAGE_MEASUREMENT_POPUP,
        s.show_storage,
        230.0,
    ));
    view.addSubview(&label(mtm, "Network speed format", 190.0));
    view.addSubview(&popup(
        mtm,
        c,
        &["Bits", "Bytes"],
        if s.network_speed_format == SpeedFormat::Bits { 0 } else { 1 },
        TAG_SPEED_FORMAT_POPUP,
        s.show_network,
        190.0,
    ));
    view
}

fn build_network_pane(mtm: MainThreadMarker, c: &PrefsWindowController, s: &Settings) -> Retained<NSView> {
    let view = container_view(mtm);
    view.addSubview(&checkbox(mtm, c, "Include public IP", s.include_public_ip, TAG_INCLUDE_PUBLIC_IP, 310.0));
    // Auto-disabled when provider is ipify — it doesn't return country data.
    let flag_enabled = s.include_public_ip && s.network_public_ip_provider != IpProvider::Ipify;
    let flag_checkbox = checkbox(mtm, c, "Show country flag", s.network_public_ip_show_flag, TAG_SHOW_FLAG, 280.0);
    flag_checkbox.setEnabled(flag_enabled);
    view.addSubview(&flag_checkbox);
    view.addSubview(&label(mtm, "IP provider", 240.0));
    let provider_index = match s.network_public_ip_provider {
        IpProvider::CoreCoding => 0,
        IpProvider::MyIp => 1,
        IpProvider::Ipify => 2,
    };
    view.addSubview(&popup(
        mtm,
        c,
        &["CoreCoding", "My-IP", "ipify"],
        provider_index,
        TAG_IP_PROVIDER_POPUP,
        s.include_public_ip,
        240.0,
    ));
    view
}

fn build_about_pane(mtm: MainThreadMarker, c: &PrefsWindowController) -> Retained<NSView> {
    let view = container_view(mtm);
    // `imageNamed` resolves against the main bundle's Resources (where
    // `pkg/build.sh`/`release.sh` copy `assets/AppIcon.icns`) — returns
    // `None` gracefully when run as a raw dev binary with no bundle, so
    // the pane just shows text-only rather than a broken image well.
    // Container is only 350pt tall (0..350) — y=340 for a 64pt-tall image
    // put its top edge at 404, past the container's own bounds and
    // straight into the segmented tab control sitting above it in the
    // window. Keep the whole logo+text block inside 0..350.
    if let Some(logo) = NSImage::imageNamed(ns_string!("AppIcon")) {
        let image_view = NSImageView::new(mtm);
        image_view.setFrame(NSRect { origin: NSPoint { x: 0.0, y: 270.0 }, size: NSSize { width: 64.0, height: 64.0 } });
        image_view.setImage(Some(&logo));
        view.addSubview(&image_view);
    }
    view.addSubview(&label(mtm, "Pantau", 240.0));
    view.addSubview(&label(mtm, &format!("Version {}", crate::updater::CURRENT_VERSION), 218.0));
    view.addSubview(&inline_link(mtm, c, "By Naufal Akbar Nugroho", TAG_AUTHOR_LINK, 196.0));
    view.addSubview(&label(mtm, "Native macOS system monitor.", 166.0));
    view.addSubview(&label(mtm, "Rewritten from vitals-gnome (GPL-2.0) — clean-room,", 136.0));
    view.addSubview(&label(mtm, "no code or assets transcribed. Licensed under MIT.", 116.0));
    // Two equal-width buttons spanning the container's full 448pt width
    // (not the 420pt label box width — that left a 28pt dead gap on the
    // right, on top of the window's own right margin, reading as
    // off-center), edge to edge with a 12pt gap between them.
    const LINK_GAP: f64 = 12.0;
    let link_width = (448.0 - LINK_GAP) / 2.0;
    view.addSubview(&link_button(mtm, c, "View on GitHub", TAG_GITHUB_LINK, 0.0, 10.0, link_width));
    view.addSubview(&link_button(
        mtm,
        c,
        "Report an Issue",
        TAG_ISSUES_LINK,
        link_width + LINK_GAP,
        10.0,
        link_width,
    ));
    view
}
