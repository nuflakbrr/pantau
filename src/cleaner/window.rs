use objc2::rc::Retained;
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertSecondButtonReturn, NSAlertThirdButtonReturn, NSAlertStyle, NSApplication, NSApplicationActivationPolicy,
    NSAttributedStringAttachmentConveniences, NSBackingStoreType, NSButton, NSButtonType,
    NSColor, NSControlStateValueOff, NSControlStateValueOn, NSCursor, NSEvent, NSFont, NSImage, NSImageView,
    NSScrollView, NSTextAlignment, NSTextAttachment, NSTextField, NSTrackingArea,
    NSTrackingAreaOptions, NSView, NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSAttributedString, NSMutableAttributedString, NSNotification,
    NSObject, NSObjectProtocol, NSPoint, NSRect, NSRunLoop, NSRunLoopCommonModes, NSSize, NSString,
    NSTimer,
};
use std::cell::{OnceCell, RefCell};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::cleaner::clean::{
    calculate_dir_size, format_bytes, get_app_specific_caches, get_browser_cache_targets,
    get_dev_cache_targets, get_system_clean_targets, get_user_clean_targets,
};
use crate::cleaner::config::{get_all_discoverable_cache_items, CleanerConfig};
use crate::cleaner::history::HistoryLogger;
use crate::cleaner::installer::scan_installer_files;
use crate::cleaner::optimize::run_optimize;
use crate::cleaner::purge::scan_project_artifacts;
use crate::cleaner::safety::PathValidator;
use crate::cleaner::status::collect_system_metrics;
use crate::cleaner::terminal::launch_in_terminal;
use crate::cleaner::touchid::{
    check_directory_access, disable_touchid, enable_touchid, enable_touchid_in_terminal,
    is_sudo_authenticated, is_touchid_configured, is_touchid_supported, request_admin_elevation,
};

fn sf_symbol_image(name: &str) -> Option<Retained<NSImage>> {
    let lookup_names: &[&str] = match name {
        "home" | "home_page" => &["house.fill", "house"],
        "clean" | "broom" => &["broom.fill", "broom", "sparkles", "wand.and.stars", "eraser.fill"],
        "uninstall" | "uninstall app" | "frisbee_disk" => &["trash.fill", "trash", "xmark.bin.fill", "record.circle", "opticaldisc"],
        "archivebox" => &["archivebox.fill", "archivebox", "shippingbox.fill"],
        "optimize" | "bolt.fill" => &["bolt.fill", "bolt"],
        "disk analyze" | "internaldrive" => &["internaldrive", "internaldrive.fill"],
        "status" | "bar_chart" | "chart.bar.fill" => &["chart.bar.fill", "chart.bar.xaxis", "chart.bar"],
        "lock.shield" => &["lock.shield.fill", "lock.shield", "touchid", "lock.fill"],
        "heart.fill" => &["heart.fill", "heart"],
        "history" | "apple.terminal" | "terminal" => &["terminal", "apple.terminal"],
        _ => &[name],
    };
    for n in lookup_names {
        if let Some(icon) = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(n), None) {
            icon.setTemplate(true);
            return Some(icon);
        }
    }
    None
}

fn sf_symbol_attachment_string(symbol: &str) -> Option<Retained<NSAttributedString>> {
    let icon = sf_symbol_image(symbol)?;
    let attach = NSTextAttachment::new();
    attach.setImage(Some(&icon));
    Some(NSAttributedString::attributedStringWithAttachment(&attach))
}

fn button_attributed_title(symbol: &str, text: &str, is_sidebar: bool) -> Retained<NSAttributedString> {
    let title = NSMutableAttributedString::new();
    if is_sidebar {
        title.appendAttributedString(&NSAttributedString::initWithString(
            NSAttributedString::alloc(),
            ns_string!("   "),
        ));
    }
    if let Some(sym_str) = sf_symbol_attachment_string(symbol) {
        title.appendAttributedString(&sym_str);
        title.appendAttributedString(&NSAttributedString::initWithString(
            NSAttributedString::alloc(),
            ns_string!("  "),
        ));
    }
    let font = if is_sidebar {
        NSFont::systemFontOfSize(13.0)
    } else {
        NSFont::boldSystemFontOfSize(13.0)
    };
    let _ = font;
    title.appendAttributedString(&NSAttributedString::initWithString(
        NSAttributedString::alloc(),
        &NSString::from_str(text),
    ));
    title.into_super()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabIndex {
    Overview = 0,
    DeepClean = 1,
    ProjectPurge = 2,
    InstallerClean = 3,
    Optimizer = 4,
    DiskAnalyzer = 5,
    LiveStatus = 6,
    TouchId = 7,
    History = 8,
}

#[derive(Debug, Clone)]
pub struct ScanTreeItem {
    pub id: usize,
    pub category: &'static str,
    pub title: String,
    pub detail: String,
    pub path: Option<PathBuf>,
    pub size_bytes: u64,
    pub is_selected: bool,
    pub is_cleanable: bool,
}

pub struct CleanerWindowIvars {
    window: OnceCell<Retained<NSWindow>>,
    root_view: OnceCell<Retained<NSView>>,
    main_content_container: RefCell<Option<Retained<NSView>>>,
    current_tab: RefCell<TabIndex>,
    items: RefCell<Vec<ScanTreeItem>>,
    is_scanning: AtomicBool,
    scan_step: std::cell::Cell<usize>,
    scan_progress: std::cell::Cell<f64>,
    scan_status_text: RefCell<String>,
    scan_logs: RefCell<Vec<String>>,
    discovered_buffer: RefCell<Vec<ScanTreeItem>>,
    is_dry_run: std::cell::Cell<bool>,
    is_whitelist_mode: std::cell::Cell<bool>,
}

#[derive(Default)]
pub struct PointerButtonIvars {
    pub is_dimmable: std::cell::Cell<bool>,
    pub is_active_item: std::cell::Cell<bool>,
}

define_class!(
    #[unsafe(super(NSButton))]
    #[thread_kind = MainThreadOnly]
    #[ivars = PointerButtonIvars]
    pub struct PointerButton;

    impl PointerButton {
        #[unsafe(method(resetCursorRects))]
        fn reset_cursor_rects(&self) {
            let bounds = self.bounds();
            self.addCursorRect_cursor(bounds, &NSCursor::pointingHandCursor());
        }

        #[unsafe(method(cursorUpdate:))]
        fn cursor_update(&self, _event: &NSEvent) {
            NSCursor::pointingHandCursor().set();
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            if self.ivars().is_dimmable.get() && !self.ivars().is_active_item.get() {
                self.setAlphaValue(1.0);
            }
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            if self.ivars().is_dimmable.get() && !self.ivars().is_active_item.get() {
                self.setAlphaValue(0.60);
            }
        }

        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            let options = NSTrackingAreaOptions::CursorUpdate
                | NSTrackingAreaOptions::MouseEnteredAndExited
                | NSTrackingAreaOptions::ActiveAlways
                | NSTrackingAreaOptions::InVisibleRect;
            let tracking = unsafe {
                NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(),
                    self.bounds(),
                    options,
                    Some(self),
                    None,
                )
            };
            self.addTrackingArea(&tracking);
        }
    }
);

impl PointerButton {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PointerButtonIvars::default());
        unsafe { msg_send![super(this), init] }
    }

    pub fn set_sidebar_style(&self, is_active: bool) {
        self.ivars().is_dimmable.set(true);
        self.ivars().is_active_item.set(is_active);
        self.setBordered(false);
        self.setAlignment(NSTextAlignment::Left);
        self.setWantsLayer(true);
        if let Some(layer) = self.layer() {
            unsafe {
                let _: () = msg_send![&*layer, setCornerRadius: 7.0f64];
                let _: () = msg_send![&*layer, setMasksToBounds: true];
                if is_active {
                    let bg = NSColor::colorWithWhite_alpha(1.0, 0.12);
                    let _: () = msg_send![&*layer, setBackgroundColor: &*bg.CGColor()];
                } else {
                    let clear = NSColor::clearColor();
                    let _: () = msg_send![&*layer, setBackgroundColor: &*clear.CGColor()];
                }
            }
        }
        self.setAlphaValue(if is_active { 1.0 } else { 0.60 });
    }
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = CleanerWindowIvars]
    pub struct CleanerWindowController;

    unsafe impl NSObjectProtocol for CleanerWindowController {}
    unsafe impl NSWindowDelegate for CleanerWindowController {}

    impl CleanerWindowController {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSNotification) {
            let mtm = self.mtm();
            let app = NSApplication::sharedApplication(mtm);
            let my_win: Option<&NSWindow> = self.ivars().window.get().map(|w| &**w);
            let mut other_visible = false;
            for win in app.windows().iter() {
                if let Some(closing) = my_win {
                    if std::ptr::eq(&*win, closing) {
                        continue;
                    }
                }
                if win.isVisible() && win.canBecomeKeyWindow() {
                    other_visible = true;
                    break;
                }
            }
            if !other_visible {
                app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            }
        }

        #[unsafe(method(windowDidResize:))]
        fn window_did_resize(&self, _notification: &NSNotification) {
            self.render_all();
        }

        #[unsafe(method(tabButtonClicked:))]
        fn tab_button_clicked(&self, sender: &NSButton) {
            let tag = sender.tag();
            let tab = match tag {
                0 => TabIndex::Overview,
                1 => TabIndex::DeepClean,
                2 => TabIndex::ProjectPurge,
                3 => TabIndex::InstallerClean,
                4 => TabIndex::Optimizer,
                5 => TabIndex::DiskAnalyzer,
                6 | 9 => TabIndex::LiveStatus,
                7 => TabIndex::TouchId,
                8 => TabIndex::History,
                _ => TabIndex::Overview,
            };
            self.ivars().is_whitelist_mode.set(false);
            *self.ivars().current_tab.borrow_mut() = tab;
            self.render_all();

            if tag == 9 {
                self.schedule_deferred_timer(sel!(showHealthModal:));
                return;
            }

            if tab == TabIndex::DeepClean || tab == TabIndex::ProjectPurge || tab == TabIndex::InstallerClean {
                self.start_live_deep_scan();
            }
        }

        #[unsafe(method(startDeferredScan:))]
        fn start_deferred_scan(&self, _timer: &NSTimer) {
            self.start_live_deep_scan();
        }

        #[unsafe(method(executeScanStep:))]
        fn execute_scan_step(&self, _timer: &NSTimer) {
            self.run_next_scan_step();
        }

        #[unsafe(method(showHealthModal:))]
        fn show_health_modal(&self, _timer: &NSTimer) {
            let metrics = collect_system_metrics();
            let alert = NSAlert::new(self.mtm());
            alert.setMessageText(ns_string!("Mac System Health Overview"));
            alert.setInformativeText(&NSString::from_str(&format!(
                "Host: {}\nmacOS: {}\nArchitecture: Apple Silicon\n\n🟢 Health Score: {}/100 ({})\n💾 Internal Storage: {} / {} ({:.1}% used)\n🧠 Memory (RAM): {} / {} ({:.1}% active)",
                metrics.hostname,
                metrics.os_version,
                metrics.health_score,
                if metrics.health_score >= 80 { "Optimal Condition" } else { "Action Recommended" },
                format_bytes(metrics.disk_used_bytes),
                format_bytes(metrics.disk_total_bytes),
                metrics.disk_used_pct,
                format_bytes(metrics.memory_used_bytes),
                format_bytes(metrics.memory_total_bytes),
                metrics.memory_used_pct
            )));
            alert.addButtonWithTitle(ns_string!("OK"));
            alert.runModal();
        }

        #[unsafe(method(triggerDeepScanNow:))]
        fn trigger_deep_scan_now(&self, _sender: &NSButton) {
            self.start_live_deep_scan();
        }

        #[unsafe(method(toggleItemCheckbox:))]
        fn toggle_item_checkbox(&self, sender: &NSButton) {
            let tag = sender.tag() as usize;
            let mut items = self.ivars().items.borrow_mut();
            if let Some(item) = items.get_mut(tag) {
                item.is_selected = sender.state() == NSControlStateValueOn;
            }
            drop(items);
            self.render_main_content();
        }

        #[unsafe(method(selectAllTapped:))]
        fn select_all_tapped(&self, _sender: &NSButton) {
            let current_tab = *self.ivars().current_tab.borrow();
            let mut items = self.ivars().items.borrow_mut();

            let filter_cat: Option<&str> = match current_tab {
                TabIndex::ProjectPurge => Some("Project Build Artifacts"),
                TabIndex::InstallerClean => Some("Raw Installers"),
                TabIndex::DeepClean => None,
                _ => None,
            };

            let all_selected = items
                .iter()
                .filter(|i| filter_cat.is_none() || Some(i.category) == filter_cat)
                .all(|i| i.is_selected);

            for item in items.iter_mut() {
                if filter_cat.is_none() || Some(item.category) == filter_cat {
                    item.is_selected = !all_selected;
                }
            }
            drop(items);
            self.render_main_content();
        }

        #[unsafe(method(cleanSelectedTapped:))]
        fn clean_selected_tapped(&self, _sender: &NSButton) {
            self.handle_clean_confirmation();
        }

        #[unsafe(method(runOptimizerTapped:))]
        fn run_optimizer_tapped(&self, _sender: &NSButton) {
            let mtm = self.mtm();
            let alert = NSAlert::new(mtm);
            alert.setMessageText(ns_string!("Run System Optimization"));
            alert.setInformativeText(ns_string!(
                "This will execute the following maintenance routines:\n\n• Flush DNS cache (mDNSResponder)\n• Purge inactive RAM memory caches\n• Rebuild LaunchServices database\n• Reset QuickLook & Font caches\n• Vacuum Safari & System SQLite DBs"
            ));
            alert.addButtonWithTitle(ns_string!("Optimize Now"));
            alert.addButtonWithTitle(ns_string!("Cancel"));
            if alert.runModal() == NSAlertFirstButtonReturn {
                let report = run_optimize(false, false);
                let done = NSAlert::new(mtm);
                done.setMessageText(ns_string!("Optimization Complete"));
                done.setInformativeText(&NSString::from_str(&format!(
                    "Successfully applied {} system maintenance optimizations.",
                    report.applied
                )));
                done.addButtonWithTitle(ns_string!("OK"));
                done.runModal();
            }
        }

        #[unsafe(method(toggleTouchIdTapped:))]
        fn toggle_touch_id_tapped(&self, _sender: &NSButton) {
            let mtm = self.mtm();
            if !is_touchid_supported() {
                let alert = NSAlert::new(mtm);
                alert.setMessageText(ns_string!("Touch ID Not Supported"));
                alert.setInformativeText(ns_string!("This Mac does not have Touch ID hardware available."));
                alert.addButtonWithTitle(ns_string!("OK"));
                alert.runModal();
                return;
            }

            let is_on = is_touchid_configured();
            if is_on {
                let _ = disable_touchid(false);
                let ok = NSAlert::new(mtm);
                ok.setMessageText(ns_string!("Disable Touch ID in Terminal"));
                ok.setInformativeText(ns_string!("Terminal has been opened to disable Touch ID for sudo. Please enter your password in Terminal if prompted."));
                ok.addButtonWithTitle(ns_string!("OK"));
                ok.runModal();
            } else {
                let _ = enable_touchid(false);
                let ok = NSAlert::new(mtm);
                ok.setMessageText(ns_string!("Enable Touch ID in Terminal"));
                ok.setInformativeText(ns_string!("Terminal has been opened to configure Touch ID for sudo. Please enter your password in Terminal to complete setup."));
                ok.addButtonWithTitle(ns_string!("OK"));
                ok.runModal();
            }
            self.render_main_content();
        }

        #[unsafe(method(dryRunTapped:))]
        fn dry_run_tapped(&self, _sender: &NSButton) {
            let next = !self.ivars().is_dry_run.get();
            self.ivars().is_dry_run.set(next);
            self.ivars().is_whitelist_mode.set(false);
            self.start_live_deep_scan();
        }

        #[unsafe(method(whitelistTapped:))]
        fn whitelist_tapped(&self, _sender: &NSButton) {
            let next = !self.ivars().is_whitelist_mode.get();
            self.ivars().is_whitelist_mode.set(next);
            self.render_main_content();
        }

        #[unsafe(method(whitelistBackTapped:))]
        fn whitelist_back_tapped(&self, _sender: &NSButton) {
            self.ivars().is_whitelist_mode.set(false);
            self.start_live_deep_scan();
        }

        #[unsafe(method(toggleWhitelistItem:))]
        fn toggle_whitelist_item(&self, sender: &NSButton) {
            let tag = sender.tag() as usize;
            let config = CleanerConfig::new();
            let mut current = config.load_whitelist();
            let all_items = get_all_discoverable_cache_items();

            if let Some(item) = all_items.get(tag) {
                let pat = &item.pattern;
                let is_checked = sender.state() == NSControlStateValueOn;
                if is_checked {
                    if !current.iter().any(|p| p == pat) {
                        current.push(pat.clone());
                    }
                } else {
                    current.retain(|p| p != pat);
                }
                let _ = config.save_whitelist(&current);
            }
            self.render_main_content();
        }

        #[unsafe(method(toggleAllWhitelistTapped:))]
        fn toggle_all_whitelist_tapped(&self, _sender: &NSButton) {
            let config = CleanerConfig::new();
            let current = config.load_whitelist();
            let all_items = get_all_discoverable_cache_items();

            let all_protected = all_items.iter().all(|item| current.iter().any(|p| p == &item.pattern));
            let new_list: Vec<String> = if all_protected {
                Vec::new()
            } else {
                all_items.iter().map(|i| i.pattern.clone()).collect()
            };
            let _ = config.save_whitelist(&new_list);
            self.render_main_content();
        }

        #[unsafe(method(launchCliFromSidebar:))]
        fn launch_cli_from_sidebar(&self, sender: &NSButton) {
            let sub = match sender.tag() {
                1 => "analyze",
                2 => "status",
                3 => "uninstall",
                _ => "",
            };
            launch_in_terminal(sub);
        }
    }
);

impl CleanerWindowController {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(CleanerWindowIvars {
            window: OnceCell::new(),
            root_view: OnceCell::new(),
            main_content_container: RefCell::new(None),
            current_tab: RefCell::new(TabIndex::Overview),
            items: RefCell::new(Vec::new()),
            is_scanning: AtomicBool::new(false),
            scan_step: std::cell::Cell::new(0),
            scan_progress: std::cell::Cell::new(0.0),
            scan_status_text: RefCell::new(String::from("Ready")),
            scan_logs: RefCell::new(Vec::new()),
            discovered_buffer: RefCell::new(Vec::new()),
            is_dry_run: std::cell::Cell::new(false),
            is_whitelist_mode: std::cell::Cell::new(false),
        });
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        this.build_window(mtm);
        this
    }

    pub fn show(&self) {
        if let Some(window) = self.ivars().window.get() {
            let mtm = self.mtm();
            let app = NSApplication::sharedApplication(mtm);
            app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);
            window.center();
            window.makeKeyAndOrderFront(None);
            window.orderFrontRegardless();
            window.enableCursorRects();
        }
        self.render_all();
        if let Some(window) = self.ivars().window.get() {
            window.resetCursorRects();
        }
    }

    pub fn show_dashboard(&self) {
        *self.ivars().current_tab.borrow_mut() = TabIndex::Overview;
        self.show();
    }

    pub fn show_deep_scan(&self) {
        *self.ivars().current_tab.borrow_mut() = TabIndex::DeepClean;
        self.show();
        self.schedule_deferred_scan();
    }

    pub fn show_project_purge(&self) {
        *self.ivars().current_tab.borrow_mut() = TabIndex::ProjectPurge;
        self.show();
        if self.ivars().items.borrow().is_empty() {
            self.schedule_deferred_scan();
        }
    }

    pub fn show_installer_clean(&self) {
        *self.ivars().current_tab.borrow_mut() = TabIndex::InstallerClean;
        self.show();
        if self.ivars().items.borrow().is_empty() {
            self.schedule_deferred_scan();
        }
    }

    pub fn schedule_deferred_scan(&self) {
        self.schedule_deferred_timer(sel!(startDeferredScan:));
    }

    fn schedule_deferred_timer(&self, selector: objc2::runtime::Sel) {
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                0.05,
                self,
                selector,
                None,
                false,
            )
        };
        unsafe {
            NSRunLoop::currentRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes);
        }
    }

    pub fn show_optimizer(&self) {
        *self.ivars().current_tab.borrow_mut() = TabIndex::Optimizer;
        self.show();
    }

    pub fn show_disk_analyzer(&self) {
        *self.ivars().current_tab.borrow_mut() = TabIndex::DiskAnalyzer;
        self.show();
    }

    pub fn show_live_status(&self) {
        *self.ivars().current_tab.borrow_mut() = TabIndex::LiveStatus;
        self.show();
    }

    pub fn show_touch_id(&self) {
        *self.ivars().current_tab.borrow_mut() = TabIndex::TouchId;
        self.show();
    }

    pub fn show_history(&self) {
        *self.ivars().current_tab.borrow_mut() = TabIndex::History;
        self.show();
    }

    fn build_window(&self, mtm: MainThreadMarker) {
        let frame = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize { width: 940.0, height: 660.0 },
        };
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;

        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                frame,
                style,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        window.setTitle(ns_string!("Pantau Cleaner — Pro System Suite"));
        window.setAcceptsMouseMovedEvents(true);
        window.setMinSize(NSSize { width: 850.0, height: 550.0 });
        unsafe { window.setReleasedWhenClosed(false) };

        let root = NSView::new(mtm);
        root.setFrame(frame);

        window.setContentView(Some(&root));
        let delegate = objc2::runtime::ProtocolObject::from_ref(self);
        window.setDelegate(Some(delegate));
        let _ = self.ivars().window.set(window);
        let _ = self.ivars().root_view.set(root);
    }

    fn render_all(&self) {
        let mtm = self.mtm();
        let Some(root) = self.ivars().root_view.get() else { return };
        for sub in root.subviews().iter() {
            sub.removeFromSuperview();
        }

        let bounds = if let Some(win) = self.ivars().window.get() {
            if let Some(cv) = win.contentView() {
                let b = cv.bounds();
                root.setFrame(b);
                b
            } else {
                root.bounds()
            }
        } else {
            root.bounds()
        };

        let total_w = bounds.size.width.max(700.0);
        let total_h = bounds.size.height.max(500.0);
        let sidebar_w = 210.0;
        let content_w = total_w - sidebar_w;

        let current_tab = *self.ivars().current_tab.borrow();
        if current_tab == TabIndex::Overview {
            self.render_hub_overview(mtm, root, total_w, total_h);
            return;
        }

        // 1. Left Sidebar View
        let sidebar = NSView::new(mtm);
        sidebar.setFrame(NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize { width: sidebar_w, height: total_h },
        });
        sidebar.setWantsLayer(true);
        if let Some(layer) = sidebar.layer() {
            unsafe {
                let bg_color = NSColor::colorWithWhite_alpha(0.0, 0.28);
                let _: () = msg_send![&*layer, setBackgroundColor: &*bg_color.CGColor()];
            }
        }

        // Sidebar Right Border Separator Line
        let sep = NSView::new(mtm);
        sep.setFrame(NSRect {
            origin: NSPoint { x: sidebar_w - 1.0, y: 0.0 },
            size: NSSize { width: 1.0, height: total_h },
        });
        sep.setWantsLayer(true);
        if let Some(layer) = sep.layer() {
            unsafe {
                let sep_color = NSColor::colorWithWhite_alpha(1.0, 0.08);
                let _: () = msg_send![&*layer, setBackgroundColor: &*sep_color.CGColor()];
            }
        }
        sidebar.addSubview(&sep);

        // App Logo
        let logo_img = NSImage::imageNamed(ns_string!("AppIcon")).or_else(|| {
            let dev_paths = [
                "assets/AppIcon.icns",
                "assets/logo.png",
                "/Applications/Pantau.app/Contents/Resources/AppIcon.icns",
            ];
            for p in dev_paths {
                if std::path::Path::new(p).exists() {
                    let ns_p = NSString::from_str(p);
                    if let Some(img) = NSImage::initWithContentsOfFile(NSImage::alloc(), &ns_p) {
                        return Some(img);
                    }
                }
            }
            None
        });

        if let Some(logo) = logo_img {
            let logo_view = NSImageView::new(mtm);
            logo_view.setFrame(NSRect {
                origin: NSPoint { x: 18.0, y: total_h - 56.0 },
                size: NSSize { width: 38.0, height: 38.0 },
            });
            logo_view.setImage(Some(&logo));
            sidebar.addSubview(&logo_view);
        }

        // App Header: Title & Version
        let app_title = NSTextField::new(mtm);
        app_title.setStringValue(ns_string!("Pantau Cleaner"));
        app_title.setBezeled(false);
        app_title.setDrawsBackground(false);
        app_title.setEditable(false);
        app_title.setSelectable(false);
        app_title.setFont(Some(&NSFont::boldSystemFontOfSize(14.5)));
        app_title.setFrame(NSRect {
            origin: NSPoint { x: 62.0, y: total_h - 40.0 },
            size: NSSize { width: sidebar_w - 70.0, height: 20.0 },
        });
        sidebar.addSubview(&app_title);

        let app_sub = NSTextField::new(mtm);
        app_sub.setStringValue(&NSString::from_str(&format!("Pantau · Version {}", env!("CARGO_PKG_VERSION"))));
        app_sub.setBezeled(false);
        app_sub.setDrawsBackground(false);
        app_sub.setEditable(false);
        app_sub.setSelectable(false);
        app_sub.setFont(Some(&NSFont::systemFontOfSize(10.5)));
        app_sub.setFrame(NSRect {
            origin: NSPoint { x: 62.0, y: total_h - 56.0 },
            size: NSSize { width: sidebar_w - 70.0, height: 16.0 },
        });
        sidebar.addSubview(&app_sub);

        // Sidebar Navigation Items
        let nav_items = [
            ("home_page", "Home", TabIndex::Overview, 0),
            ("broom", "Clean", TabIndex::DeepClean, 1),
            ("frisbee_disk", "Uninstall App", TabIndex::InstallerClean, 3),
            ("bolt.fill", "Optimize", TabIndex::Optimizer, 4),
            ("internaldrive", "Disk Analyzer", TabIndex::DiskAnalyzer, 5),
            ("bar_chart", "Status", TabIndex::LiveStatus, 6),
            ("apple.terminal", "History Logs", TabIndex::History, 8),
        ];

        let btn_h = 36.0;
        let spacing = 6.0;
        let mut start_y = total_h - 116.0;

        for (sym, label, tab, tag) in nav_items {
            let is_active = tab == current_tab;
            let btn = PointerButton::new(mtm);
            btn.setButtonType(NSButtonType::MomentaryPushIn);
            let font = if is_active {
                NSFont::boldSystemFontOfSize(12.5)
            } else {
                NSFont::systemFontOfSize(12.0)
            };
            btn.setFont(Some(&font));
            let attr = button_attributed_title(sym, label, true);
            btn.setAttributedTitle(&attr);
            btn.set_sidebar_style(is_active);
            btn.setTag(tag);
            btn.setFrame(NSRect {
                origin: NSPoint { x: 12.0, y: start_y },
                size: NSSize { width: sidebar_w - 24.0, height: btn_h },
            });
            unsafe {
                btn.setTarget(Some(self));
                btn.setAction(Some(sel!(tabButtonClicked:)));
            }
            sidebar.addSubview(&btn);
            start_y -= btn_h + spacing;
        }

        // Sidebar Bottom: Touch ID (right above Open with Terminal)
        let is_touchid_active = current_tab == TabIndex::TouchId;
        let touchid_btn = PointerButton::new(mtm);
        touchid_btn.setButtonType(NSButtonType::MomentaryPushIn);
        let font = if is_touchid_active {
            NSFont::boldSystemFontOfSize(12.5)
        } else {
            NSFont::systemFontOfSize(12.0)
        };
        touchid_btn.setFont(Some(&font));
        let touchid_attr = button_attributed_title("lock.shield", "Touch ID", true);
        touchid_btn.setAttributedTitle(&touchid_attr);
        touchid_btn.set_sidebar_style(is_touchid_active);
        touchid_btn.setTag(7);
        touchid_btn.setFrame(NSRect {
            origin: NSPoint { x: 12.0, y: 58.0 },
            size: NSSize { width: sidebar_w - 24.0, height: 36.0 },
        });
        unsafe {
            touchid_btn.setTarget(Some(self));
            touchid_btn.setAction(Some(sel!(tabButtonClicked:)));
        }
        sidebar.addSubview(&touchid_btn);

        // Sidebar Bottom: Open with Terminal
        let cli_box_btn = PointerButton::new(mtm);
        cli_box_btn.setButtonType(NSButtonType::MomentaryPushIn);
        cli_box_btn.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        let cli_attr = button_attributed_title("apple.terminal", "Open with Terminal", true);
        cli_box_btn.setAttributedTitle(&cli_attr);
        cli_box_btn.set_sidebar_style(false);
        cli_box_btn.setTag(0);
        cli_box_btn.setFrame(NSRect {
            origin: NSPoint { x: 12.0, y: 16.0 },
            size: NSSize { width: sidebar_w - 24.0, height: 36.0 },
        });
        unsafe {
            cli_box_btn.setTarget(Some(self));
            cli_box_btn.setAction(Some(sel!(launchCliFromSidebar:)));
        }
        sidebar.addSubview(&cli_box_btn);

        root.addSubview(&sidebar);

        // 2. Main Content Container
        let content_container = NSView::new(mtm);
        content_container.setFrame(NSRect {
            origin: NSPoint { x: sidebar_w, y: 0.0 },
            size: NSSize { width: content_w, height: total_h },
        });
        root.addSubview(&content_container);
        *self.ivars().main_content_container.borrow_mut() = Some(content_container);

        self.render_main_content();
        if let Some(win) = self.ivars().window.get() {
            win.display();
            win.invalidateCursorRectsForView(root);
            win.resetCursorRects();
        }
    }

    fn render_main_content(&self) {
        let mtm = self.mtm();
        let content_holder = self.ivars().main_content_container.borrow().clone();
        let Some(content) = content_holder else { return };
        for sub in content.subviews().iter() {
            sub.removeFromSuperview();
        }

        let tab = *self.ivars().current_tab.borrow();
        match tab {
            TabIndex::Overview => {},
            TabIndex::DeepClean => self.render_tree_tab(mtm, &content, None, "Disk & Cache Cleaner"),
            TabIndex::ProjectPurge => self.render_tree_tab(mtm, &content, Some("Project Build Artifacts"), "Project Build Artifacts Purger"),
            TabIndex::InstallerClean => self.render_tree_tab(mtm, &content, Some("Raw Installers"), "Raw Installer Cleaner (.dmg / .pkg / .zip)"),
            TabIndex::Optimizer => self.render_optimizer_tab(mtm, &content),
            TabIndex::DiskAnalyzer => self.render_analyzer_tab(mtm, &content),
            TabIndex::LiveStatus => self.render_live_status_tab(mtm, &content),
            TabIndex::TouchId => self.render_touch_id_tab(mtm, &content),
            TabIndex::History => self.render_history_tab(mtm, &content),
        }
    }

    // -------------------------------------------------------------------------
    // TAB 0: OVERVIEW HUB (MINIMALIST DASHBOARD)
    // -------------------------------------------------------------------------
    fn render_hub_overview(&self, mtm: MainThreadMarker, root: &NSView, total_w: f64, total_h: f64) {
        let cluster_h = 440.0;
        let cluster_top = ((total_h + cluster_h) / 2.0).min(total_h - 20.0);

        // 1. App Icon / Logo
        let logo_img = NSImage::imageNamed(ns_string!("AppIcon")).or_else(|| {
            let dev_paths = [
                "assets/AppIcon.icns",
                "assets/logo.png",
                "/Applications/Pantau.app/Contents/Resources/AppIcon.icns",
            ];
            for p in dev_paths {
                if std::path::Path::new(p).exists() {
                    let ns_p = NSString::from_str(p);
                    if let Some(img) = NSImage::initWithContentsOfFile(NSImage::alloc(), &ns_p) {
                        return Some(img);
                    }
                }
            }
            None
        });

        let logo_size = 110.0;
        let logo_y = cluster_top - logo_size;
        if let Some(logo) = logo_img {
            let logo_view = NSImageView::new(mtm);
            logo_view.setFrame(NSRect {
                origin: NSPoint {
                    x: (total_w - logo_size) / 2.0,
                    y: logo_y,
                },
                size: NSSize {
                    width: logo_size,
                    height: logo_size,
                },
            });
            logo_view.setImage(Some(&logo));
            root.addSubview(&logo_view);
        }

        // 2. Title: Pantau Cleaner
        let title_y = logo_y - 14.0 - 30.0;
        let title = NSTextField::new(mtm);
        title.setStringValue(ns_string!("Pantau Cleaner"));
        title.setFont(Some(&NSFont::boldSystemFontOfSize(24.0)));
        title.setBezeled(false);
        title.setEditable(false);
        title.setDrawsBackground(false);
        title.setSelectable(false);
        title.setAlignment(NSTextAlignment::Center);
        title.setFrame(NSRect {
            origin: NSPoint {
                x: 40.0,
                y: title_y,
            },
            size: NSSize {
                width: total_w - 80.0,
                height: 30.0,
            },
        });
        root.addSubview(&title);

        // Subtitle: Pantau · Version X.Y.Z
        let subtitle_y = title_y - 4.0 - 18.0;
        let subtitle = NSTextField::new(mtm);
        subtitle.setStringValue(&NSString::from_str(&format!("Pantau · Version {}", env!("CARGO_PKG_VERSION"))));
        subtitle.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        subtitle.setBezeled(false);
        subtitle.setEditable(false);
        subtitle.setDrawsBackground(false);
        subtitle.setSelectable(false);
        subtitle.setAlignment(NSTextAlignment::Center);
        subtitle.setFrame(NSRect {
            origin: NSPoint {
                x: 40.0,
                y: subtitle_y,
            },
            size: NSSize {
                width: total_w - 80.0,
                height: 18.0,
            },
        });
        root.addSubview(&subtitle);

        // 3. Multi-line description paragraph
        let desc_y = subtitle_y - 12.0 - 68.0;
        let desc = NSTextField::new(mtm);
        desc.setStringValue(ns_string!(
            "Pantau Cleaner is an all-in-one, ultra-fast system maintenance and optimization suite built in pure Rust for macOS.\nClean user and system caches, purge project build artifacts, remove forgotten installer images,\noptimize system memory & DNS, and inspect disk usage with zero overhead."
        ));
        desc.setFont(Some(&NSFont::systemFontOfSize(12.5)));
        desc.setBezeled(false);
        desc.setEditable(false);
        desc.setDrawsBackground(false);
        desc.setSelectable(false);
        desc.setAlignment(NSTextAlignment::Center);
        desc.setFrame(NSRect {
            origin: NSPoint {
                x: 60.0,
                y: desc_y,
            },
            size: NSSize {
                width: total_w - 120.0,
                height: 68.0,
            },
        });
        root.addSubview(&desc);

        // 4. Feature Action Buttons Grid (Matching Sidebar Features)
        // Row 1 (3 items)
        let row1_items = [
            ("broom", "Clean", 1),
            ("frisbee_disk", "Uninstall App", 3),
            ("bolt.fill", "Optimize", 4),
        ];
        let r1_count = row1_items.len() as f64;
        let r1_btn_w = 175.0;
        let r1_gap = 14.0;
        let r1_total_w = r1_count * r1_btn_w + (r1_count - 1.0) * r1_gap;
        let r1_start_x = (total_w - r1_total_w) / 2.0;
        let r1_y = desc_y - 28.0 - 42.0;

        for (idx, (sym, label, tag)) in row1_items.iter().enumerate() {
            let btn = PointerButton::new(mtm);
            btn.setButtonType(NSButtonType::MomentaryPushIn);
            btn.setFont(Some(&NSFont::systemFontOfSize(13.0)));
            let attr = button_attributed_title(sym, label, false);
            btn.setAttributedTitle(&attr);
            btn.setTag(*tag);
            let b_frame = NSRect {
                origin: NSPoint {
                    x: r1_start_x + (idx as f64) * (r1_btn_w + r1_gap),
                    y: r1_y,
                },
                size: NSSize {
                    width: r1_btn_w,
                    height: 42.0,
                },
            };
            btn.setFrame(b_frame);
            unsafe {
                btn.setTarget(Some(self));
                btn.setAction(Some(sel!(tabButtonClicked:)));
            }
            root.addSubview(&btn);
        }

        // Row 2 (3 items)
        let row2_items = [
            ("internaldrive", "Disk Analyzer", 5),
            ("bar_chart", "Status", 6),
            ("apple.terminal", "History Logs", 8),
        ];
        let r2_count = row2_items.len() as f64;
        let r2_btn_w = 175.0;
        let r2_gap = 14.0;
        let r2_total_w = r2_count * r2_btn_w + (r2_count - 1.0) * r2_gap;
        let r2_start_x = (total_w - r2_total_w) / 2.0;
        let r2_y = r1_y - 12.0 - 42.0;

        for (idx, (sym, label, tag)) in row2_items.iter().enumerate() {
            let btn = PointerButton::new(mtm);
            btn.setButtonType(NSButtonType::MomentaryPushIn);
            btn.setFont(Some(&NSFont::systemFontOfSize(13.0)));
            let attr = button_attributed_title(sym, label, false);
            btn.setAttributedTitle(&attr);
            btn.setTag(*tag);
            let b_frame = NSRect {
                origin: NSPoint {
                    x: r2_start_x + (idx as f64) * (r2_btn_w + r2_gap),
                    y: r2_y,
                },
                size: NSSize {
                    width: r2_btn_w,
                    height: 42.0,
                },
            };
            btn.setFrame(b_frame);
            unsafe {
                btn.setTarget(Some(self));
                btn.setAction(Some(sel!(tabButtonClicked:)));
            }
            root.addSubview(&btn);
        }

        // 5. Bottom Action: Launch with Terminal (clean text link)
        let term_y = r2_y - 20.0 - 24.0;
        let term_btn = PointerButton::new(mtm);
        term_btn.setBordered(false);
        term_btn.setAlignment(NSTextAlignment::Center);
        term_btn.setFont(Some(&NSFont::systemFontOfSize(13.0)));
        let term_attr = button_attributed_title("apple.terminal", "Launch with Terminal", false);
        term_btn.setAttributedTitle(&term_attr);
        term_btn.setTag(0);
        let term_rect = NSRect {
            origin: NSPoint {
                x: (total_w - 200.0) / 2.0,
                y: term_y,
            },
            size: NSSize {
                width: 200.0,
                height: 24.0,
            },
        };
        term_btn.setFrame(term_rect);
        unsafe {
            term_btn.setTarget(Some(self));
            term_btn.setAction(Some(sel!(launchCliFromSidebar:)));
        }
        root.addSubview(&term_btn);

        if let Some(win) = self.ivars().window.get() {
            win.invalidateCursorRectsForView(root);
            win.resetCursorRects();
        }
    }

    // -------------------------------------------------------------------------
    // WHITELIST MANAGER VIEW (IN-APP INTERACTIVE TOGGLE)
    // -------------------------------------------------------------------------
    fn render_whitelist_manager_view(&self, mtm: MainThreadMarker, parent: &NSView) {
        let bounds = parent.bounds();
        let content_w = bounds.size.width;
        let content_h = bounds.size.height;
        let card_w = (content_w - 40.0).max(400.0);
        let card_h = (content_h - 125.0).max(200.0);
        let card_y = 60.0;

        let config = CleanerConfig::new();
        let current_wl = config.load_whitelist();
        let all_items = get_all_discoverable_cache_items();
        let protected_count = all_items.iter().filter(|i| {
            let i_trim = i.pattern.trim_end_matches("/*").trim_end_matches('*');
            current_wl.iter().any(|p| {
                let p_trim = p.trim_end_matches("/*").trim_end_matches('*');
                p == &i.pattern || p_trim == i_trim || i.pattern.starts_with(p_trim) || p.starts_with(i_trim)
            })
        }).count();

        // 1. Header Bar
        let header = NSView::new(mtm);
        header.setFrame(NSRect {
            origin: NSPoint { x: 20.0, y: content_h - 55.0 },
            size: NSSize { width: card_w, height: 45.0 },
        });

        let title_lbl = NSTextField::new(mtm);
        let title_attr = button_attributed_title("lock.shield", "Protected Whitelist Manager", false);
        title_lbl.setAttributedStringValue(&title_attr);
        title_lbl.setFont(Some(&NSFont::boldSystemFontOfSize(16.5)));
        title_lbl.setBezeled(false); title_lbl.setEditable(false); title_lbl.setDrawsBackground(false);
        title_lbl.setFrame(NSRect { origin: NSPoint { x: 0.0, y: 10.0 }, size: NSSize { width: (card_w - 260.0).max(180.0), height: 26.0 } });
        header.addSubview(&title_lbl);

        let btn_y = 6.0;
        let btn_h = 32.0;

        let back_btn = PointerButton::new(mtm);
        back_btn.setTitle(ns_string!("← Back to Scan"));
        back_btn.setButtonType(NSButtonType::MomentaryPushIn);
        back_btn.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
        back_btn.setFrame(NSRect { origin: NSPoint { x: card_w - 130.0, y: btn_y }, size: NSSize { width: 130.0, height: btn_h } });
        unsafe {
            back_btn.setTarget(Some(self));
            back_btn.setAction(Some(sel!(whitelistBackTapped:)));
        }
        header.addSubview(&back_btn);

        let toggle_all_btn = PointerButton::new(mtm);
        toggle_all_btn.setTitle(ns_string!("Toggle All"));
        toggle_all_btn.setButtonType(NSButtonType::MomentaryPushIn);
        toggle_all_btn.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        toggle_all_btn.setFrame(NSRect { origin: NSPoint { x: card_w - 235.0, y: btn_y }, size: NSSize { width: 95.0, height: btn_h } });
        unsafe {
            toggle_all_btn.setTarget(Some(self));
            toggle_all_btn.setAction(Some(sel!(toggleAllWhitelistTapped:)));
        }
        header.addSubview(&toggle_all_btn);

        parent.addSubview(&header);

        // 2. Central Scroll List
        let scroll = NSScrollView::new(mtm);
        scroll.setFrame(NSRect {
            origin: NSPoint { x: 20.0, y: card_y },
            size: NSSize { width: card_w, height: card_h },
        });
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setWantsLayer(true);
        if let Some(layer) = scroll.layer() {
            unsafe {
                let bg = NSColor::colorWithWhite_alpha(0.0, 0.40);
                let _: () = msg_send![&*layer, setBackgroundColor: &*bg.CGColor()];
                let _: () = msg_send![&*layer, setCornerRadius: 8.0f64];
                let _: () = msg_send![&*layer, setMasksToBounds: true];
                let border = NSColor::colorWithWhite_alpha(1.0, 0.08);
                let _: () = msg_send![&*layer, setBorderColor: &*border.CGColor()];
                let _: () = msg_send![&*layer, setBorderWidth: 1.0f64];
            }
        }

        let content = NSView::new(mtm);
        let mono_font = NSFont::monospacedSystemFontOfSize_weight(11.5, unsafe { objc2_app_kit::NSFontWeightRegular });
        let bold_font = NSFont::boldSystemFontOfSize(12.5);
        let regular_font = NSFont::systemFontOfSize(12.5);
        let green_color = NSColor::colorWithRed_green_blue_alpha(0.3, 0.85, 0.45, 1.0);
        let gray_color = NSColor::colorWithWhite_alpha(1.0, 0.3);
        let pat_color = NSColor::colorWithWhite_alpha(1.0, 0.45);

        let row_h = 32.0;
        let total_h = (all_items.len() as f64 * row_h) + 40.0;

        content.setFrame(NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize { width: card_w - 18.0, height: total_h.max(card_h - 10.0) },
        });

        let mut current_y = total_h - 26.0;

        for (idx, item) in all_items.iter().enumerate() {
            let is_protected = current_wl.iter().any(|p| {
                let p_trim = p.trim_end_matches("/*").trim_end_matches('*');
                let i_trim = item.pattern.trim_end_matches("/*").trim_end_matches('*');
                p == &item.pattern || p_trim == i_trim || item.pattern.starts_with(p_trim) || p.starts_with(i_trim)
            });

            let row = NSView::new(mtm);
            row.setFrame(NSRect {
                origin: NSPoint { x: 12.0, y: current_y - row_h },
                size: NSSize { width: card_w - 40.0, height: row_h },
            });

            let chk = PointerButton::new(mtm);
            chk.setButtonType(NSButtonType::Switch);
            chk.setTitle(ns_string!(""));
            chk.setState(if is_protected { NSControlStateValueOn } else { NSControlStateValueOff });
            chk.setTag(idx as isize);
            chk.setFrame(NSRect { origin: NSPoint { x: 4.0, y: 6.0 }, size: NSSize { width: 22.0, height: 20.0 } });
            unsafe {
                chk.setTarget(Some(self));
                chk.setAction(Some(sel!(toggleWhitelistItem:)));
            }
            row.addSubview(&chk);

            let bullet = NSTextField::new(mtm);
            bullet.setStringValue(if is_protected { ns_string!("●") } else { ns_string!("○") });
            bullet.setFont(Some(&bold_font));
            bullet.setTextColor(Some(if is_protected {
                &green_color
            } else {
                &gray_color
            }));
            bullet.setBezeled(false); bullet.setEditable(false); bullet.setDrawsBackground(false);
            bullet.setFrame(NSRect { origin: NSPoint { x: 28.0, y: 5.0 }, size: NSSize { width: 16.0, height: 20.0 } });
            row.addSubview(&bullet);

            let name_lbl = NSTextField::new(mtm);
            name_lbl.setStringValue(&NSString::from_str(&item.display_name));
            name_lbl.setFont(Some(if is_protected { &bold_font } else { &regular_font }));
            name_lbl.setBezeled(false); name_lbl.setEditable(false); name_lbl.setDrawsBackground(false);
            name_lbl.setFrame(NSRect { origin: NSPoint { x: 46.0, y: 6.0 }, size: NSSize { width: 320.0, height: 20.0 } });
            row.addSubview(&name_lbl);

            let pat_lbl = NSTextField::new(mtm);
            pat_lbl.setStringValue(&NSString::from_str(&item.pattern));
            pat_lbl.setFont(Some(&mono_font));
            pat_lbl.setTextColor(Some(&pat_color));
            pat_lbl.setBezeled(false); pat_lbl.setEditable(false); pat_lbl.setDrawsBackground(false);
            pat_lbl.setFrame(NSRect { origin: NSPoint { x: 375.0, y: 6.0 }, size: NSSize { width: (card_w - 430.0).max(100.0), height: 20.0 } });
            row.addSubview(&pat_lbl);

            content.addSubview(&row);
            current_y -= row_h;
        }

        scroll.setDocumentView(Some(&content));
        parent.addSubview(&scroll);

        // 3. Bottom Summary Strip
        let bottom_bar = NSView::new(mtm);
        bottom_bar.setFrame(NSRect {
            origin: NSPoint { x: 20.0, y: 8.0 },
            size: NSSize { width: card_w, height: 36.0 },
        });

        let status_lbl = NSTextField::new(mtm);
        let status_msg = format!("🛡️ {} of {} cache items protected in ~/.config/pantau/whitelist", protected_count, all_items.len());
        status_lbl.setStringValue(&NSString::from_str(&status_msg));
        status_lbl.setFont(Some(&mono_font));
        status_lbl.setBezeled(false); status_lbl.setEditable(false); status_lbl.setDrawsBackground(false);
        status_lbl.setFrame(NSRect { origin: NSPoint { x: 0.0, y: 8.0 }, size: NSSize { width: card_w - 180.0, height: 20.0 } });
        bottom_bar.addSubview(&status_lbl);

        let done_btn = PointerButton::new(mtm);
        done_btn.setTitle(ns_string!("Done & Scan"));
        done_btn.setButtonType(NSButtonType::MomentaryPushIn);
        done_btn.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
        done_btn.setFrame(NSRect { origin: NSPoint { x: card_w - 140.0, y: 2.0 }, size: NSSize { width: 140.0, height: 30.0 } });
        unsafe {
            done_btn.setTarget(Some(self));
            done_btn.setAction(Some(sel!(whitelistBackTapped:)));
        }
        bottom_bar.addSubview(&done_btn);

        parent.addSubview(&bottom_bar);
    }

    // -------------------------------------------------------------------------
    // TAB 1 / 2 / 3: SCAN TREE VIEW (DEEP CLEAN / PROJECT PURGE / INSTALLER)
    // -------------------------------------------------------------------------
    fn render_tree_tab(&self, mtm: MainThreadMarker, parent: &NSView, filter_category: Option<&str>, header_title: &str) {
        let bounds = parent.bounds();
        let content_w = bounds.size.width;
        let content_h = bounds.size.height;
        let card_w = (content_w - 40.0).max(400.0);
        let card_h = (content_h - 125.0).max(200.0);
        let card_y = 60.0;

        let is_scanning = self.ivars().is_scanning.load(Ordering::SeqCst);
        let progress = self.ivars().scan_progress.get().clamp(0.0, 1.0);
        let is_dry = self.ivars().is_dry_run.get();
        let is_wl_mode = self.ivars().is_whitelist_mode.get();

        if is_wl_mode {
            self.render_whitelist_manager_view(mtm, parent);
            return;
        }

        // 1. Top Header Bar (Title + Action Pills)
        let header = NSView::new(mtm);
        header.setFrame(NSRect {
            origin: NSPoint { x: 20.0, y: content_h - 55.0 },
            size: NSSize { width: card_w, height: 45.0 },
        });

        let title_lbl = NSTextField::new(mtm);
        let sym_name = match filter_category {
            Some("Project artifacts") => "archivebox",
            Some("Raw Installers") => "frisbee_disk",
            _ => "broom",
        };
        let title_attr = button_attributed_title(sym_name, header_title, false);
        title_lbl.setAttributedStringValue(&title_attr);
        title_lbl.setFont(Some(&NSFont::boldSystemFontOfSize(16.5)));
        title_lbl.setBezeled(false);
        title_lbl.setEditable(false);
        title_lbl.setDrawsBackground(false);
        title_lbl.setSelectable(false);
        title_lbl.setFrame(NSRect { origin: NSPoint { x: 0.0, y: 10.0 }, size: NSSize { width: (card_w - 320.0).max(180.0), height: 26.0 } });
        header.addSubview(&title_lbl);

        // Action Pills: Dry Run, Whitelist, Re-Scan
        let btn_y = 6.0;
        let btn_h = 32.0;

        let rescan_btn = PointerButton::new(mtm);
        rescan_btn.setTitle(ns_string!("Re-Scan"));
        rescan_btn.setButtonType(NSButtonType::MomentaryPushIn);
        rescan_btn.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        rescan_btn.setEnabled(!is_scanning);
        rescan_btn.setFrame(NSRect { origin: NSPoint { x: card_w - 80.0, y: btn_y }, size: NSSize { width: 80.0, height: btn_h } });
        unsafe {
            rescan_btn.setTarget(Some(self));
            rescan_btn.setAction(Some(sel!(triggerDeepScanNow:)));
        }
        header.addSubview(&rescan_btn);

        let whitelist_btn = PointerButton::new(mtm);
        whitelist_btn.setTitle(ns_string!("Whitelist"));
        whitelist_btn.setButtonType(NSButtonType::MomentaryPushIn);
        whitelist_btn.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        whitelist_btn.setFrame(NSRect { origin: NSPoint { x: card_w - 175.0, y: btn_y }, size: NSSize { width: 85.0, height: btn_h } });
        unsafe {
            whitelist_btn.setTarget(Some(self));
            whitelist_btn.setAction(Some(sel!(whitelistTapped:)));
        }
        header.addSubview(&whitelist_btn);

        let dry_btn = PointerButton::new(mtm);
        let dry_font_bold = NSFont::boldSystemFontOfSize(12.0);
        let dry_font_reg = NSFont::systemFontOfSize(12.0);
        dry_btn.setTitle(if is_dry { ns_string!("Dry Run: ON") } else { ns_string!("Dry Run") });
        dry_btn.setButtonType(NSButtonType::MomentaryPushIn);
        dry_btn.setFont(Some(if is_dry { &dry_font_bold } else { &dry_font_reg }));
        dry_btn.setFrame(NSRect { origin: NSPoint { x: card_w - 295.0, y: btn_y }, size: NSSize { width: 110.0, height: btn_h } });
        unsafe {
            dry_btn.setTarget(Some(self));
            dry_btn.setAction(Some(sel!(dryRunTapped:)));
        }
        header.addSubview(&dry_btn);

        parent.addSubview(&header);

        // 2. Central Terminal / Console Card Area
        let scroll = NSScrollView::new(mtm);
        scroll.setFrame(NSRect {
            origin: NSPoint { x: 20.0, y: card_y },
            size: NSSize { width: card_w, height: card_h },
        });
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setWantsLayer(true);
        if let Some(layer) = scroll.layer() {
            unsafe {
                let bg = NSColor::colorWithWhite_alpha(0.0, 0.40);
                let _: () = msg_send![&*layer, setBackgroundColor: &*bg.CGColor()];
                let _: () = msg_send![&*layer, setCornerRadius: 8.0f64];
                let _: () = msg_send![&*layer, setMasksToBounds: true];
                let border = NSColor::colorWithWhite_alpha(1.0, 0.08);
                let _: () = msg_send![&*layer, setBorderColor: &*border.CGColor()];
                let _: () = msg_send![&*layer, setBorderWidth: 1.0f64];
            }
        }

        let content = NSView::new(mtm);
        let mono_font = NSFont::monospacedSystemFontOfSize_weight(
            12.0,
            unsafe { objc2_app_kit::NSFontWeightRegular },
        );
        let mono_bold = NSFont::monospacedSystemFontOfSize_weight(
            12.5,
            unsafe { objc2_app_kit::NSFontWeightBold },
        );

        if is_scanning {
            // =================================================================
            // LIVE INTERACTIVE TERMINAL SCANNING STREAM (MOLE STYLE)
            // =================================================================
            let logs = self.ivars().scan_logs.borrow();
            let total_h = (logs.len() as f64 * 22.0) + 50.0;

            content.setFrame(NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize { width: card_w - 18.0, height: total_h.max(card_h - 10.0) },
            });

            let mut current_y = total_h - 22.0;
            for (idx, line) in logs.iter().enumerate() {
                let lbl = NSTextField::new(mtm);
                let is_last = idx == logs.len() - 1;
                let text_val = if is_last {
                    format!("{} ▋", line)
                } else {
                    line.clone()
                };

                lbl.setStringValue(&NSString::from_str(&text_val));
                if line.starts_with("Clean Your Mac") {
                    lbl.setFont(Some(&NSFont::boldSystemFontOfSize(15.0)));
                } else if line.starts_with("➤") {
                    lbl.setFont(Some(&mono_bold));
                } else {
                    lbl.setFont(Some(&mono_font));
                }

                lbl.setBezeled(false);
                lbl.setEditable(false);
                lbl.setDrawsBackground(false);
                lbl.setFrame(NSRect {
                    origin: NSPoint { x: 16.0, y: current_y - 18.0 },
                    size: NSSize { width: card_w - 36.0, height: 18.0 },
                });
                content.addSubview(&lbl);
                current_y -= 22.0;
            }
        } else {
            // =================================================================
            // POST-SCAN INTERACTIVE TREE VIEW WITH CHECKBOXES
            // =================================================================
            let items = self.ivars().items.borrow();
            let canonical_sections: &[&str] = if let Some(cat) = filter_category {
                match cat {
                    "Project artifacts" => &["Project artifacts"],
                    "Raw Installers" => &["Raw Installers"],
                    _ => &["User essentials"],
                }
            } else {
                &[
                    "User essentials",
                    "App caches",
                    "Browsers",
                    "Cloud & Office",
                    "Developer tools",
                    "Apps & utilities",
                    "Virtualization",
                    "Application Support",
                    "App leftovers",
                    "Apple Silicon updates",
                    "Device backups & firmware",
                    "Time Machine",
                    "Large files",
                    "Project artifacts",
                    "Raw Installers",
                    "System",
                ]
            };

            let filtered_items: Vec<(usize, &ScanTreeItem)> = items
                .iter()
                .enumerate()
                .filter(|(_, item)| filter_category.is_none() || Some(item.category) == filter_category)
                .collect();

            let row_height = 26.0;
            let cat_header_h = 28.0;
            let empty_row_h = 24.0;
            let header_lines_h = 160.0;
            let footer_summary_h = 100.0;

            let mut total_rows_h = 0.0;
            for &sec in canonical_sections {
                total_rows_h += cat_header_h;
                let sec_items_count = filtered_items.iter().filter(|(_, it)| it.category == sec).count();
                if sec_items_count > 0 {
                    total_rows_h += sec_items_count as f64 * row_height;
                } else {
                    total_rows_h += empty_row_h;
                }
                total_rows_h += 8.0;
            }

            let total_h = header_lines_h + total_rows_h + footer_summary_h;

            content.setFrame(NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize { width: card_w - 18.0, height: total_h.max(card_h - 10.0) },
            });

            let mut current_y = total_h - 20.0;

            // Terminal Top Banner: Clean Your Mac
            let title_terminal = NSTextField::new(mtm);
            title_terminal.setStringValue(ns_string!("Clean Your Mac with Pantau Cleaner"));
            title_terminal.setFont(Some(&NSFont::boldSystemFontOfSize(15.0)));
            title_terminal.setBezeled(false); title_terminal.setEditable(false); title_terminal.setDrawsBackground(false);
            title_terminal.setFrame(NSRect { origin: NSPoint { x: 16.0, y: current_y - 20.0 }, size: NSSize { width: card_w - 40.0, height: 20.0 } });
            content.addSubview(&title_terminal);
            current_y -= 26.0;

            // Sudo & Touch ID Session Indicator
            let is_root = unsafe { libc::geteuid() == 0 };
            let is_sudo = is_root || is_sudo_authenticated();
            let touchid_configured = is_touchid_configured();
            let sudo_line = NSTextField::new(mtm);
            if is_sudo {
                if touchid_configured {
                    sudo_line.setStringValue(ns_string!("✓ Touch ID verified | Admin access available, system preview included"));
                } else {
                    sudo_line.setStringValue(ns_string!("✓ Admin access available (password verified), system preview included"));
                }
            } else {
                sudo_line.setStringValue(ns_string!("◎ System caches need sudo, run sudo -v for full preview"));
            }
            sudo_line.setFont(Some(&mono_font));
            sudo_line.setBezeled(false); sudo_line.setEditable(false); sudo_line.setDrawsBackground(false);
            sudo_line.setFrame(NSRect { origin: NSPoint { x: 16.0, y: current_y - 18.0 }, size: NSSize { width: card_w - 40.0, height: 18.0 } });
            content.addSubview(&sudo_line);
            current_y -= 22.0;

            // Architecture & Free Space
            let info_line = NSTextField::new(mtm);
            let metrics = collect_system_metrics();
            info_line.setStringValue(&NSString::from_str(&format!("⚙ Apple Silicon | Free space: {}", format_bytes(metrics.disk_total_bytes.saturating_sub(metrics.disk_used_bytes)))));
            info_line.setFont(Some(&mono_font));
            info_line.setBezeled(false); info_line.setEditable(false); info_line.setDrawsBackground(false);
            info_line.setFrame(NSRect { origin: NSPoint { x: 16.0, y: current_y - 18.0 }, size: NSSize { width: card_w - 40.0, height: 18.0 } });
            content.addSubview(&info_line);
            current_y -= 20.0;

            // Whitelist Indicator
            let wl_line = NSTextField::new(mtm);
            let config = CleanerConfig::new();
            let wl_count = config.load_whitelist().len();
            wl_line.setStringValue(&NSString::from_str(&format!("✓ Whitelist: {} core patterns active", wl_count)));
            wl_line.setFont(Some(&mono_font));
            wl_line.setBezeled(false); wl_line.setEditable(false); wl_line.setDrawsBackground(false);
            wl_line.setFrame(NSRect { origin: NSPoint { x: 16.0, y: current_y - 18.0 }, size: NSSize { width: card_w - 40.0, height: 18.0 } });
            content.addSubview(&wl_line);
            current_y -= 22.0;

            // Divider
            let div1 = NSTextField::new(mtm);
            div1.setStringValue(ns_string!("──────────────────────────────────────────────────────────────────────────────────────────"));
            div1.setFont(Some(&mono_font));
            div1.setBezeled(false); div1.setEditable(false); div1.setDrawsBackground(false);
            div1.setFrame(NSRect { origin: NSPoint { x: 16.0, y: current_y - 14.0 }, size: NSSize { width: card_w - 32.0, height: 14.0 } });
            content.addSubview(&div1);
            current_y -= 22.0;

            // Render Canonical Mole Sections
            let mut active_category_count = 0usize;
            for &sec in canonical_sections {
                let sec_items: Vec<(usize, &ScanTreeItem)> = filtered_items.iter().filter(|(_, it)| it.category == sec).cloned().collect();

                let cat_lbl = NSTextField::new(mtm);
                cat_lbl.setStringValue(&NSString::from_str(&format!("➤ {}", sec)));
                cat_lbl.setFont(Some(&mono_bold));
                cat_lbl.setBezeled(false); cat_lbl.setEditable(false); cat_lbl.setDrawsBackground(false);
                cat_lbl.setFrame(NSRect { origin: NSPoint { x: 16.0, y: current_y - 18.0 }, size: NSSize { width: card_w - 40.0, height: 18.0 } });
                content.addSubview(&cat_lbl);
                current_y -= cat_header_h;

                if !sec_items.is_empty() {
                    active_category_count += 1;
                    for (orig_idx, item) in sec_items {
                        let row = NSView::new(mtm);
                        row.setFrame(NSRect { origin: NSPoint { x: 16.0, y: current_y - row_height }, size: NSSize { width: card_w - 48.0, height: row_height } });

                        let chk = PointerButton::new(mtm);
                        chk.setButtonType(NSButtonType::Switch);
                        chk.setTitle(ns_string!(""));
                        chk.setState(if item.is_selected { NSControlStateValueOn } else { NSControlStateValueOff });
                        chk.setTag(orig_idx as isize);
                        chk.setFrame(NSRect { origin: NSPoint { x: 4.0, y: 3.0 }, size: NSSize { width: 20.0, height: 20.0 } });
                        unsafe {
                            chk.setTarget(Some(self));
                            chk.setAction(Some(sel!(toggleItemCheckbox:)));
                        }
                        row.addSubview(&chk);

                        let item_title = NSTextField::new(mtm);
                        item_title.setStringValue(&NSString::from_str(&format!("→ {}", item.title)));
                        item_title.setFont(Some(&mono_font));
                        item_title.setBezeled(false); item_title.setEditable(false); item_title.setDrawsBackground(false);
                        item_title.setFrame(NSRect { origin: NSPoint { x: 26.0, y: 3.0 }, size: NSSize { width: 280.0, height: 20.0 } });
                        row.addSubview(&item_title);

                        let item_stat = NSTextField::new(mtm);
                        item_stat.setStringValue(&NSString::from_str(&format!("· {}", item.detail)));
                        item_stat.setFont(Some(&mono_font));
                        item_stat.setBezeled(false); item_stat.setEditable(false); item_stat.setDrawsBackground(false);
                        item_stat.setFrame(NSRect { origin: NSPoint { x: 305.0, y: 3.0 }, size: NSSize { width: card_w - 450.0, height: 20.0 } });
                        row.addSubview(&item_stat);

                        let sz_lbl = NSTextField::new(mtm);
                        sz_lbl.setStringValue(&NSString::from_str(&format_bytes(item.size_bytes)));
                        sz_lbl.setFont(Some(&mono_bold));
                        sz_lbl.setBezeled(false); sz_lbl.setEditable(false); sz_lbl.setDrawsBackground(false);
                        sz_lbl.setAlignment(NSTextAlignment::Right);
                        sz_lbl.setFrame(NSRect { origin: NSPoint { x: card_w - 180.0, y: 3.0 }, size: NSSize { width: 120.0, height: 20.0 } });
                        row.addSubview(&sz_lbl);

                        content.addSubview(&row);
                        current_y -= row_height;
                    }
                } else {
                    let clean_row = NSTextField::new(mtm);
                    clean_row.setStringValue(ns_string!("  ✓ Nothing to clean"));
                    clean_row.setFont(Some(&mono_font));
                    clean_row.setBezeled(false); clean_row.setEditable(false); clean_row.setDrawsBackground(false);
                    clean_row.setFrame(NSRect { origin: NSPoint { x: 16.0, y: current_y - 16.0 }, size: NSSize { width: card_w - 40.0, height: 16.0 } });
                    content.addSubview(&clean_row);
                    current_y -= empty_row_h;
                }
                current_y -= 8.0;
            }

            // Bottom Summary Box within Console
            current_y -= 10.0;
            let div_summary1 = NSTextField::new(mtm);
            div_summary1.setStringValue(ns_string!("================================================================================──────────"));
            div_summary1.setFont(Some(&mono_font));
            div_summary1.setBezeled(false); div_summary1.setEditable(false); div_summary1.setDrawsBackground(false);
            div_summary1.setFrame(NSRect { origin: NSPoint { x: 16.0, y: current_y - 14.0 }, size: NSSize { width: card_w - 32.0, height: 14.0 } });
            content.addSubview(&div_summary1);
            current_y -= 20.0;

            let _selected_count = filtered_items.iter().filter(|(_, i)| i.is_selected).count();
            let selected_bytes: u64 = filtered_items.iter().filter(|(_, i)| i.is_selected).map(|(_, i)| i.size_bytes).sum();

            let sum_lbl = NSTextField::new(mtm);
            sum_lbl.setStringValue(&NSString::from_str(&format!(
                "Potential space: {} | Items: {} | Categories: {}",
                format_bytes(selected_bytes),
                filtered_items.len(),
                active_category_count
            )));
            sum_lbl.setFont(Some(&mono_bold));
            sum_lbl.setBezeled(false); sum_lbl.setEditable(false); sum_lbl.setDrawsBackground(false);
            sum_lbl.setFrame(NSRect { origin: NSPoint { x: 16.0, y: current_y - 18.0 }, size: NSSize { width: card_w - 40.0, height: 18.0 } });
            content.addSubview(&sum_lbl);
            current_y -= 20.0;

            let div_summary2 = NSTextField::new(mtm);
            div_summary2.setStringValue(ns_string!("================================================================================──────────"));
            div_summary2.setFont(Some(&mono_font));
            div_summary2.setBezeled(false); div_summary2.setEditable(false); div_summary2.setDrawsBackground(false);
            div_summary2.setFrame(NSRect { origin: NSPoint { x: 16.0, y: current_y - 14.0 }, size: NSSize { width: card_w - 32.0, height: 14.0 } });
            content.addSubview(&div_summary2);
        }

        scroll.setDocumentView(Some(&content));
        parent.addSubview(&scroll);

        // 3. Dynamic Progress Bar View
        let track_bar = NSView::new(mtm);
        track_bar.setFrame(NSRect {
            origin: NSPoint { x: 20.0, y: 46.0 },
            size: NSSize { width: card_w, height: 7.0 },
        });
        track_bar.setWantsLayer(true);
        if let Some(t_layer) = track_bar.layer() {
            unsafe {
                let track_bg = NSColor::colorWithWhite_alpha(1.0, 0.09);
                let _: () = msg_send![&*t_layer, setBackgroundColor: &*track_bg.CGColor()];
                let _: () = msg_send![&*t_layer, setCornerRadius: 3.5f64];
                let _: () = msg_send![&*t_layer, setMasksToBounds: true];
            }
        }

        let fill_w = ((card_w * progress).max(if progress > 0.01 { 6.0 } else { 0.0 })).min(card_w);
        let fill_bar = NSView::new(mtm);
        fill_bar.setFrame(NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize { width: fill_w, height: 7.0 },
        });
        fill_bar.setWantsLayer(true);
        if let Some(f_layer) = fill_bar.layer() {
            unsafe {
                let fill_color = NSColor::colorWithRed_green_blue_alpha(0.32, 0.58, 0.98, 1.0);
                let _: () = msg_send![&*f_layer, setBackgroundColor: &*fill_color.CGColor()];
                let _: () = msg_send![&*f_layer, setCornerRadius: 3.5f64];
            }
        }
        track_bar.addSubview(&fill_bar);
        parent.addSubview(&track_bar);

        // 4. Bottom Status & Actions Strip
        let bottom_bar = NSView::new(mtm);
        bottom_bar.setFrame(NSRect {
            origin: NSPoint { x: 20.0, y: 8.0 },
            size: NSSize { width: card_w, height: 32.0 },
        });

        if is_scanning {
            let status_lbl = NSTextField::new(mtm);
            let status_msg = format!("⌛ {}", self.ivars().scan_status_text.borrow());
            status_lbl.setStringValue(&NSString::from_str(&status_msg));
            status_lbl.setFont(Some(&NSFont::monospacedSystemFontOfSize_weight(11.5, unsafe { objc2_app_kit::NSFontWeightRegular })));
            status_lbl.setBezeled(false); status_lbl.setEditable(false); status_lbl.setDrawsBackground(false);
            status_lbl.setFrame(NSRect { origin: NSPoint { x: 0.0, y: 6.0 }, size: NSSize { width: card_w - 90.0, height: 20.0 } });
            bottom_bar.addSubview(&status_lbl);

            let pct_lbl = NSTextField::new(mtm);
            let pct_str = format!("{}%", (progress * 100.0) as u32);
            pct_lbl.setStringValue(&NSString::from_str(&pct_str));
            pct_lbl.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
            pct_lbl.setBezeled(false); pct_lbl.setEditable(false); pct_lbl.setDrawsBackground(false);
            pct_lbl.setAlignment(NSTextAlignment::Right);
            pct_lbl.setFrame(NSRect { origin: NSPoint { x: card_w - 70.0, y: 6.0 }, size: NSSize { width: 65.0, height: 20.0 } });
            bottom_bar.addSubview(&pct_lbl);
        } else {
            let items = self.ivars().items.borrow();
            let filtered_items: Vec<(usize, &ScanTreeItem)> = items
                .iter()
                .enumerate()
                .filter(|(_, item)| filter_category.is_none() || Some(item.category) == filter_category)
                .collect();
            let _selected_count = filtered_items.iter().filter(|(_, i)| i.is_selected).count();
            let selected_bytes: u64 = filtered_items.iter().filter(|(_, i)| i.is_selected).map(|(_, i)| i.size_bytes).sum();

            let toggle_all = PointerButton::new(mtm);
            toggle_all.setTitle(ns_string!("Toggle All"));
            toggle_all.setButtonType(NSButtonType::MomentaryPushIn);
            toggle_all.setFont(Some(&NSFont::systemFontOfSize(11.5)));
            toggle_all.setFrame(NSRect { origin: NSPoint { x: 0.0, y: 2.0 }, size: NSSize { width: 90.0, height: 28.0 } });
            unsafe {
                toggle_all.setTarget(Some(self));
                toggle_all.setAction(Some(sel!(selectAllTapped:)));
            }
            bottom_bar.addSubview(&toggle_all);

            let is_dry = self.ivars().is_dry_run.get();
            let clean_btn = PointerButton::new(mtm);
            let btn_title = if is_dry {
                format!("Simulate Dry Run ({})", format_bytes(selected_bytes))
            } else {
                format!("Clean Selected ({})", format_bytes(selected_bytes))
            };
            clean_btn.setTitle(&NSString::from_str(&btn_title));
            clean_btn.setButtonType(NSButtonType::MomentaryPushIn);
            clean_btn.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
            clean_btn.setEnabled(selected_bytes > 0);
            clean_btn.setFrame(NSRect { origin: NSPoint { x: 100.0, y: 2.0 }, size: NSSize { width: 230.0, height: 28.0 } });
            unsafe {
                clean_btn.setTarget(Some(self));
                clean_btn.setAction(Some(sel!(cleanSelectedTapped:)));
            }
            bottom_bar.addSubview(&clean_btn);
        }

        parent.addSubview(&bottom_bar);
    }

    // -------------------------------------------------------------------------
    // TAB 4: OPTIMIZER
    // -------------------------------------------------------------------------
    fn render_optimizer_tab(&self, mtm: MainThreadMarker, parent: &NSView) {
        let bounds = parent.bounds();
        let content_w = bounds.size.width;
        let content_h = bounds.size.height;

        let title = NSTextField::new(mtm);
        let title_attr = button_attributed_title("bolt.fill", "System Optimizer & Maintenance", false);
        title.setAttributedStringValue(&title_attr);
        title.setFont(Some(&NSFont::boldSystemFontOfSize(16.0)));
        title.setBezeled(false); title.setEditable(false); title.setDrawsBackground(false);
        title.setFrame(NSRect { origin: NSPoint { x: 20.0, y: content_h - 55.0 }, size: NSSize { width: content_w - 40.0, height: 24.0 } });
        parent.addSubview(&title);

        let desc = NSTextField::new(mtm);
        desc.setStringValue(ns_string!("Keep your Mac fast and responsive by running automated system maintenance routines."));
        desc.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        desc.setBezeled(false); desc.setEditable(false); desc.setDrawsBackground(false);
        desc.setFrame(NSRect { origin: NSPoint { x: 20.0, y: content_h - 80.0 }, size: NSSize { width: content_w - 40.0, height: 18.0 } });
        parent.addSubview(&desc);

        let routines = [
            ("Flush DNS Cache", "Restarts mDNSResponder to resolve internet domain lookup lag.", true),
            ("Purge Inactive Memory", "Reclaims inactive and speculative RAM pages using native kernel calls.", true),
            ("Rebuild LaunchServices Database", "Fixes duplicate 'Open With' application entries and wrong file icons.", true),
            ("Reset QuickLook Thumbnail Cache", "Clears broken file preview cache in Finder.", true),
            ("Reset System Font Databases", "Fixes font rendering glitches and corrupt font caches.", true),
            ("Vacuum SQLite Databases", "Compacts and speeds up Safari, Mail, and system internal databases.", true),
        ];

        let mut y = content_h - 145.0;
        for (name, detail, _) in routines {
            let row = NSView::new(mtm);
            row.setFrame(NSRect { origin: NSPoint { x: 20.0, y }, size: NSSize { width: (content_w - 40.0).min(650.0), height: 50.0 } });

            let t = NSTextField::new(mtm);
            t.setStringValue(&NSString::from_str(name));
            t.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
            t.setBezeled(false); t.setEditable(false); t.setDrawsBackground(false);
            t.setFrame(NSRect { origin: NSPoint { x: 0.0, y: 26.0 }, size: NSSize { width: (content_w - 60.0).min(630.0), height: 20.0 } });
            row.addSubview(&t);

            let d = NSTextField::new(mtm);
            d.setStringValue(&NSString::from_str(detail));
            d.setFont(Some(&NSFont::systemFontOfSize(11.0)));
            d.setBezeled(false); d.setEditable(false); d.setDrawsBackground(false);
            d.setFrame(NSRect { origin: NSPoint { x: 0.0, y: 4.0 }, size: NSSize { width: (content_w - 60.0).min(630.0), height: 18.0 } });
            row.addSubview(&d);

            parent.addSubview(&row);
            y -= 58.0;
        }

        let opt_btn = PointerButton::new(mtm);
        opt_btn.setButtonType(NSButtonType::MomentaryPushIn);
        opt_btn.setFont(Some(&NSFont::boldSystemFontOfSize(13.5)));
        let opt_attr = button_attributed_title("bolt.fill", "Run All System Optimizations", false);
        opt_btn.setAttributedTitle(&opt_attr);
        opt_btn.setFrame(NSRect { origin: NSPoint { x: 20.0, y: 35.0 }, size: NSSize { width: 280.0, height: 36.0 } });
        unsafe {
            opt_btn.setTarget(Some(self));
            opt_btn.setAction(Some(sel!(runOptimizerTapped:)));
        }
        parent.addSubview(&opt_btn);
    }

    // -------------------------------------------------------------------------
    // TAB 5: DISK ANALYZER
    // -------------------------------------------------------------------------
    fn render_analyzer_tab(&self, mtm: MainThreadMarker, parent: &NSView) {
        let bounds = parent.bounds();
        let content_w = bounds.size.width;
        let content_h = bounds.size.height;

        let title = NSTextField::new(mtm);
        let title_attr = button_attributed_title("internaldrive", "Visual Disk Space Analyzer", false);
        title.setAttributedStringValue(&title_attr);
        title.setFont(Some(&NSFont::boldSystemFontOfSize(16.0)));
        title.setBezeled(false); title.setEditable(false); title.setDrawsBackground(false);
        title.setFrame(NSRect { origin: NSPoint { x: 20.0, y: content_h - 55.0 }, size: NSSize { width: content_w - 40.0, height: 24.0 } });
        parent.addSubview(&title);

        let desc = NSTextField::new(mtm);
        desc.setStringValue(ns_string!("Interactive visual terminal analyzer for discovering largest files and folders across your drives."));
        desc.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        desc.setBezeled(false); desc.setEditable(false); desc.setDrawsBackground(false);
        desc.setFrame(NSRect { origin: NSPoint { x: 20.0, y: content_h - 80.0 }, size: NSSize { width: content_w - 40.0, height: 20.0 } });
        parent.addSubview(&desc);

        let launch_btn = PointerButton::new(mtm);
        launch_btn.setButtonType(NSButtonType::MomentaryPushIn);
        launch_btn.setTag(1);
        launch_btn.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
        let launch_attr = button_attributed_title("internaldrive", "Launch Interactive Disk Analyzer (TUI)", false);
        launch_btn.setAttributedTitle(&launch_attr);
        launch_btn.setFrame(NSRect { origin: NSPoint { x: 20.0, y: content_h - 140.0 }, size: NSSize { width: 340.0, height: 36.0 } });
        unsafe {
            launch_btn.setTarget(Some(self));
            launch_btn.setAction(Some(sel!(launchCliFromSidebar:)));
        }
        parent.addSubview(&launch_btn);
    }

    // -------------------------------------------------------------------------
    // TAB 6: LIVE SYSTEM STATUS
    // -------------------------------------------------------------------------
    fn render_live_status_tab(&self, mtm: MainThreadMarker, parent: &NSView) {
        let bounds = parent.bounds();
        let content_w = bounds.size.width;
        let content_h = bounds.size.height;

        let title = NSTextField::new(mtm);
        let title_attr = button_attributed_title("bar_chart", "Live System Status & Process Watcher", false);
        title.setAttributedStringValue(&title_attr);
        title.setFont(Some(&NSFont::boldSystemFontOfSize(16.0)));
        title.setBezeled(false); title.setEditable(false); title.setDrawsBackground(false);
        title.setFrame(NSRect { origin: NSPoint { x: 20.0, y: content_h - 55.0 }, size: NSSize { width: content_w - 40.0, height: 24.0 } });
        parent.addSubview(&title);

        let metrics = collect_system_metrics();

        let card_w = ((content_w - 60.0) / 2.0).max(260.0);
        let card_h = 100.0;
        let row1_y = content_h - 170.0;
        let row2_y = row1_y - 120.0;

        let host_card = self.make_metric_card(mtm, "Host & macOS", &metrics.hostname, &format!("{} · Score: {}/100", metrics.os_version, metrics.health_score), 20.0, row1_y, card_w, card_h);
        parent.addSubview(&host_card);

        let cpu_card = self.make_metric_card(mtm, "CPU & Load", &format!("{:.1}%", metrics.cpu_total_pct), "Load Average", 30.0 + card_w, row1_y, card_w, card_h);
        parent.addSubview(&cpu_card);

        let disk_card = self.make_metric_card(
            mtm,
            "Internal Storage",
            &format!("{} Free", format_bytes(metrics.disk_total_bytes.saturating_sub(metrics.disk_used_bytes))),
            &format!("Used: {} / {} ({:.1}%)", format_bytes(metrics.disk_used_bytes), format_bytes(metrics.disk_total_bytes), metrics.disk_used_pct),
            20.0, row2_y, card_w, card_h,
        );
        parent.addSubview(&disk_card);

        let mem_card = self.make_metric_card(
            mtm,
            "Unified Memory (RAM)",
            &format!("{:.1}% Used", metrics.memory_used_pct),
            &format!("Active: {} / {}", format_bytes(metrics.memory_used_bytes), format_bytes(metrics.memory_total_bytes)),
            30.0 + card_w, row2_y, card_w, card_h,
        );
        parent.addSubview(&mem_card);
    }

    fn make_metric_card(&self, mtm: MainThreadMarker, title: &str, value: &str, subtitle: &str, x: f64, y: f64, w: f64, h: f64) -> Retained<NSView> {
        let card = NSView::new(mtm);
        card.setFrame(NSRect { origin: NSPoint { x, y }, size: NSSize { width: w, height: h } });
        card.setWantsLayer(true);
        if let Some(layer) = card.layer() {
            unsafe {
                let bg = NSColor::colorWithWhite_alpha(1.0, 0.05);
                let _: () = msg_send![&*layer, setBackgroundColor: &*bg.CGColor()];
                let _: () = msg_send![&*layer, setCornerRadius: 10.0f64];
            }
        }

        let t = NSTextField::new(mtm);
        t.setStringValue(&NSString::from_str(title));
        t.setFont(Some(&NSFont::systemFontOfSize(11.5)));
        t.setBezeled(false); t.setEditable(false); t.setDrawsBackground(false);
        t.setFrame(NSRect { origin: NSPoint { x: 14.0, y: h - 28.0 }, size: NSSize { width: w - 28.0, height: 18.0 } });
        card.addSubview(&t);

        let v = NSTextField::new(mtm);
        v.setStringValue(&NSString::from_str(value));
        v.setFont(Some(&NSFont::boldSystemFontOfSize(18.0)));
        v.setBezeled(false); v.setEditable(false); v.setDrawsBackground(false);
        v.setFrame(NSRect { origin: NSPoint { x: 14.0, y: h - 56.0 }, size: NSSize { width: w - 28.0, height: 26.0 } });
        card.addSubview(&v);

        let s = NSTextField::new(mtm);
        s.setStringValue(&NSString::from_str(subtitle));
        s.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        s.setBezeled(false); s.setEditable(false); s.setDrawsBackground(false);
        s.setFrame(NSRect { origin: NSPoint { x: 14.0, y: 8.0 }, size: NSSize { width: w - 28.0, height: 16.0 } });
        card.addSubview(&s);

        card
    }

    // -------------------------------------------------------------------------
    // TAB 7: TOUCH ID
    // -------------------------------------------------------------------------
    fn render_touch_id_tab(&self, mtm: MainThreadMarker, parent: &NSView) {
        let bounds = parent.bounds();
        let content_w = bounds.size.width;
        let content_h = bounds.size.height;

        let title = NSTextField::new(mtm);
        let title_attr = button_attributed_title("touchid", "Touch ID Sudo Authentication", false);
        title.setAttributedStringValue(&title_attr);
        title.setFont(Some(&NSFont::boldSystemFontOfSize(16.0)));
        title.setBezeled(false); title.setEditable(false); title.setDrawsBackground(false);
        title.setFrame(NSRect { origin: NSPoint { x: 20.0, y: content_h - 55.0 }, size: NSSize { width: content_w - 40.0, height: 24.0 } });
        parent.addSubview(&title);

        let desc = NSTextField::new(mtm);
        desc.setStringValue(ns_string!("Use your Touch ID fingerprint for sudo authorization in Terminal and pam_tid authentication."));
        desc.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        desc.setBezeled(false); desc.setEditable(false); desc.setDrawsBackground(false);
        desc.setFrame(NSRect { origin: NSPoint { x: 20.0, y: content_h - 80.0 }, size: NSSize { width: content_w - 40.0, height: 18.0 } });
        parent.addSubview(&desc);

        let card = NSView::new(mtm);
        card.setFrame(NSRect { origin: NSPoint { x: 20.0, y: content_h - 260.0 }, size: NSSize { width: (content_w - 40.0).min(650.0), height: 160.0 } });
        card.setWantsLayer(true);
        if let Some(layer) = card.layer() {
            unsafe {
                let bg = NSColor::colorWithWhite_alpha(1.0, 0.05);
                let _: () = msg_send![&*layer, setBackgroundColor: &*bg.CGColor()];
                let _: () = msg_send![&*layer, setCornerRadius: 10.0f64];
            }
        }

        let is_configured = is_touchid_configured();
        let status_title = NSTextField::new(mtm);
        status_title.setStringValue(&NSString::from_str(if is_configured {
            "🟢 Touch ID for Sudo is ACTIVE"
        } else {
            "⚪ Touch ID for Sudo is INACTIVE"
        }));
        status_title.setFont(Some(&NSFont::boldSystemFontOfSize(14.0)));
        status_title.setBezeled(false); status_title.setEditable(false); status_title.setDrawsBackground(false);
        status_title.setFrame(NSRect { origin: NSPoint { x: 18.0, y: 115.0 }, size: NSSize { width: 400.0, height: 22.0 } });
        card.addSubview(&status_title);

        let status_desc = NSTextField::new(mtm);
        status_desc.setStringValue(&NSString::from_str(if is_configured {
            "Terminal prompts will use biometric Touch ID by default."
        } else {
            "Standard password prompts are currently used for sudo elevated commands in Terminal."
        }));
        status_desc.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        status_desc.setBezeled(false); status_desc.setEditable(false); status_desc.setDrawsBackground(false);
        status_desc.setFrame(NSRect { origin: NSPoint { x: 18.0, y: 70.0 }, size: NSSize { width: 560.0, height: 35.0 } });
        card.addSubview(&status_desc);

        let toggle_btn = PointerButton::new(mtm);
        toggle_btn.setTitle(&NSString::from_str(if is_configured {
            "Disable Touch ID for Sudo"
        } else {
            "Enable Touch ID for Sudo"
        }));
        toggle_btn.setButtonType(NSButtonType::MomentaryPushIn);
        toggle_btn.setFont(Some(&NSFont::boldSystemFontOfSize(12.5)));
        toggle_btn.setFrame(NSRect { origin: NSPoint { x: 18.0, y: 18.0 }, size: NSSize { width: 220.0, height: 34.0 } });
        unsafe {
            toggle_btn.setTarget(Some(self));
            toggle_btn.setAction(Some(sel!(toggleTouchIdTapped:)));
        }
        card.addSubview(&toggle_btn);

        parent.addSubview(&card);
    }

    // -------------------------------------------------------------------------
    // TAB 8: HISTORY LOGS
    // -------------------------------------------------------------------------
    fn render_history_tab(&self, mtm: MainThreadMarker, parent: &NSView) {
        let bounds = parent.bounds();
        let content_w = bounds.size.width;
        let content_h = bounds.size.height;

        let title = NSTextField::new(mtm);
        let title_attr = button_attributed_title("apple.terminal", "Cleaner Activity & Audit Logs", false);
        title.setAttributedStringValue(&title_attr);
        title.setFont(Some(&NSFont::boldSystemFontOfSize(16.0)));
        title.setBezeled(false); title.setEditable(false); title.setDrawsBackground(false);
        title.setFrame(NSRect { origin: NSPoint { x: 20.0, y: content_h - 55.0 }, size: NSSize { width: content_w - 40.0, height: 24.0 } });
        parent.addSubview(&title);

        let card_w = (content_w - 40.0).max(300.0);
        let card_h = (content_h - 85.0).max(200.0);

        let scroll = NSScrollView::new(mtm);
        scroll.setFrame(NSRect {
            origin: NSPoint { x: 20.0, y: 20.0 },
            size: NSSize { width: card_w, height: card_h },
        });
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);

        let logger = HistoryLogger::new();
        let ops = logger.read_recent_operations(40);

        let total_h = (ops.len() as f64 * 44.0) + 30.0;
        let content = NSView::new(mtm);
        content.setFrame(NSRect { origin: NSPoint { x: 0.0, y: 0.0 }, size: NSSize { width: card_w - 20.0, height: total_h.max(card_h - 10.0) } });

        let mut current_y = total_h - 25.0;
        for op in ops {
            current_y -= 40.0;
            let row = NSView::new(mtm);
            row.setFrame(NSRect { origin: NSPoint { x: 8.0, y: current_y }, size: NSSize { width: card_w - 36.0, height: 38.0 } });

            let t = NSTextField::new(mtm);
            t.setStringValue(&NSString::from_str(&format!("{} • {} ({})", op.timestamp, op.command, op.target)));
            t.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
            t.setBezeled(false); t.setEditable(false); t.setDrawsBackground(false);
            t.setFrame(NSRect { origin: NSPoint { x: 0.0, y: 16.0 }, size: NSSize { width: (card_w - 200.0).max(200.0), height: 18.0 } });
            row.addSubview(&t);

            let sz = NSTextField::new(mtm);
            sz.setStringValue(&NSString::from_str(&format!("Size: {}", format_bytes(op.size_bytes))));
            sz.setFont(Some(&NSFont::systemFontOfSize(11.5)));
            sz.setBezeled(false); sz.setEditable(false); sz.setDrawsBackground(false);
            sz.setAlignment(objc2_app_kit::NSTextAlignment::Right);
            sz.setFrame(NSRect { origin: NSPoint { x: card_w - 180.0, y: 16.0 }, size: NSSize { width: 140.0, height: 18.0 } });
            row.addSubview(&sz);

            content.addSubview(&row);
        }

        scroll.setDocumentView(Some(&content));
        parent.addSubview(&scroll);
    }

    // -------------------------------------------------------------------------
    // SCAN ENGINE
    // -------------------------------------------------------------------------
    fn check_and_request_device_access(&self) -> Option<(bool, bool)> {
        let is_root = unsafe { libc::geteuid() == 0 };
        let mut is_authed = is_root || is_sudo_authenticated();
        let touchid_configured = is_touchid_configured();

        if !is_authed {
            if touchid_configured {
                // macOS automatically prompts the native biometric Touch ID modal
                let ok = request_admin_elevation("Touch ID verification required to scan system caches and protected logs");
                if ok {
                    is_authed = is_sudo_authenticated();
                } else {
                    return None;
                }
            } else {
                // Informative modal dialog requesting admin credentials or suggesting Touch ID
                let mtm = self.mtm();
                let alert = NSAlert::new(mtm);
                alert.setAlertStyle(NSAlertStyle::Informational);
                alert.setMessageText(ns_string!("Administrator Deep Scan Access"));
                alert.setInformativeText(ns_string!(
                    "Deep clean scanning inspects system-level caches, developer tools, and protected application logs.\n\nTouch ID for Sudo is currently INACTIVE. You can authenticate with your device password, or enable Touch ID for 1-touch fingerprint scans."
                ));
                alert.addButtonWithTitle(ns_string!("Authenticate with Password"));
                alert.addButtonWithTitle(ns_string!("Enable Touch ID"));
                alert.addButtonWithTitle(ns_string!("Continue as Standard User"));

                let response = alert.runModal();
                if response == NSAlertFirstButtonReturn {
                    let ok = request_admin_elevation("Enter your administrator password to unlock full system scan");
                    if ok {
                        is_authed = is_sudo_authenticated();
                    } else {
                        return None;
                    }
                } else if response == NSAlertSecondButtonReturn {
                    enable_touchid_in_terminal();
                    let info = NSAlert::new(mtm);
                    info.setMessageText(ns_string!("Touch ID Configuration in Terminal"));
                    info.setInformativeText(ns_string!(
                        "Terminal has been opened to configure Touch ID for sudo.\n\nPlease enter your password in the Terminal window, then click 'Continue Scan' below."
                    ));
                    info.addButtonWithTitle(ns_string!("Continue Scan"));
                    info.addButtonWithTitle(ns_string!("Cancel"));
                    let sub_resp = info.runModal();
                    if sub_resp == NSAlertFirstButtonReturn {
                        if is_touchid_configured() {
                            let ok = request_admin_elevation("Touch ID verification required to scan system caches and protected logs");
                            if ok {
                                is_authed = is_sudo_authenticated();
                            } else {
                                return None;
                            }
                        } else {
                            let incomplete = NSAlert::new(mtm);
                            incomplete.setMessageText(ns_string!("Touch ID Setup Incomplete"));
                            incomplete.setInformativeText(ns_string!(
                                "Touch ID configuration was not detected in Terminal.\n\nPlease complete entering your password in Terminal and try scanning again."
                            ));
                            incomplete.addButtonWithTitle(ns_string!("OK"));
                            incomplete.runModal();
                            return None;
                        }
                    } else {
                        return None;
                    }
                } else if response == NSAlertThirdButtonReturn {
                    is_authed = false;
                } else {
                    return None;
                }
            }
        }

        Some((is_authed, is_touchid_configured()))
    }

    pub fn start_live_deep_scan(&self) {
        if self.ivars().is_scanning.swap(true, Ordering::SeqCst) {
            return;
        }

        let (is_authed, touchid_configured) = match self.check_and_request_device_access() {
            Some(res) => res,
            None => {
                self.ivars().is_scanning.store(false, Ordering::SeqCst);
                self.render_main_content();
                return;
            }
        };

        self.ivars().scan_step.set(0);
        self.ivars().scan_progress.set(0.05);
        *self.ivars().scan_status_text.borrow_mut() = "Initializing deep cache scanner...".to_string();
        self.ivars().discovered_buffer.borrow_mut().clear();

        let metrics = collect_system_metrics();
        let free_space = format_bytes(metrics.disk_total_bytes.saturating_sub(metrics.disk_used_bytes));
        let config = CleanerConfig::new();
        let wl_count = config.load_whitelist().len();

        let sudo_str = if is_authed {
            if touchid_configured {
                "✓ Touch ID verified | Admin access available, system preview included".to_string()
            } else {
                "✓ Admin access available (password verified), system preview included".to_string()
            }
        } else {
            "◎ System caches need sudo, run sudo -v for full preview".to_string()
        };

        let mut initial_logs = Vec::new();
        let is_dry = self.ivars().is_dry_run.get();
        if is_dry {
            initial_logs.push("Clean Your Mac (DRY RUN SIMULATION)".to_string());
            initial_logs.push("[DRY RUN ACTIVE - Files and caches will only be simulated without deletion]".to_string());
        } else {
            initial_logs.push("Clean Your Mac with Pantau Cleaner".to_string());
        }
        initial_logs.push(sudo_str);
        initial_logs.push(format!("⚙ Apple Silicon | Free space: {}", free_space));
        initial_logs.push(format!("✓ Whitelist: {} core patterns active", wl_count));

        // Folder access verification check
        let dir_access = check_directory_access();
        let all_dirs_ok = dir_access.iter().all(|(_, ok)| *ok);
        if all_dirs_ok {
            initial_logs.push("✓ Folder access verified for User Caches, Application Support & Containers".to_string());
        } else {
            let accessible_names: Vec<&str> = dir_access.iter().filter(|(_, ok)| *ok).map(|(n, _)| *n).collect();
            initial_logs.push(format!("✓ Folder access verified: {}", accessible_names.join(", ")));
        }

        initial_logs.push("──────────────────────────────────────────────────────────────────────────────────────────".to_string());
        initial_logs.push("➤ Starting deep scan...".to_string());

        *self.ivars().scan_logs.borrow_mut() = initial_logs;

        self.render_main_content();
        self.schedule_scan_step_timer(0.08);
    }

    fn schedule_scan_step_timer(&self, interval: f64) {
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                interval,
                self,
                sel!(executeScanStep:),
                None,
                false,
            )
        };
        unsafe {
            NSRunLoop::currentRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes);
        }
    }

    fn run_next_scan_step(&self) {
        let step = self.ivars().scan_step.get();
        let mut logs = self.ivars().scan_logs.borrow_mut();
        let mut buffer = self.ivars().discovered_buffer.borrow_mut();
        let mut id_counter = buffer.len();

        match step {
            0 => {
                logs.push("➤ User essentials".to_string());
                let mut found = false;
                for target in get_user_clean_targets() {
                    for path in &target.paths {
                        let sz = calculate_dir_size(path);
                        if sz > 0 {
                            buffer.push(ScanTreeItem {
                                id: id_counter,
                                category: "User essentials",
                                title: target.name.to_string(),
                                detail: path.to_string_lossy().to_string(),
                                path: Some(path.clone()),
                                size_bytes: sz,
                                is_selected: true,
                                is_cleanable: true,
                            });
                            id_counter += 1;
                            found = true;
                            logs.push(format!("  → {} · {}", target.name, format_bytes(sz)));
                        }
                    }
                }
                if !found {
                    logs.push("  ✓ Nothing to clean".to_string());
                }
                self.ivars().scan_progress.set(0.16);
                *self.ivars().scan_status_text.borrow_mut() = "Scanning user essentials & logs...".to_string();
            }
            1 => {
                logs.push("➤ App caches".to_string());
                let app_cache_targets = [
                    ("Geod temp files", dirs_home_path("Library/Caches/GeoServices")),
                    ("macOS Help system cache", dirs_home_path("Library/Caches/com.apple.helpd")),
                    ("Apple Media Services cache", dirs_home_path("Library/Caches/com.apple.AppleMediaServices")),
                    ("Parsecd cache", dirs_home_path("Library/Caches/com.apple.parsecd")),
                    ("Group Containers logs/caches", dirs_home_path("Library/Group Containers")),
                    ("QuickLook thumbnail cache", dirs_home_path("Library/Caches/com.apple.QuickLook.thumbnailcache")),
                ];
                let mut found = false;
                for (name, path) in app_cache_targets {
                    if path.exists() {
                        let sz = calculate_dir_size(&path);
                        if sz > 0 {
                            buffer.push(ScanTreeItem {
                                id: id_counter,
                                category: "App caches",
                                title: name.to_string(),
                                detail: path.to_string_lossy().to_string(),
                                path: Some(path),
                                size_bytes: sz,
                                is_selected: true,
                                is_cleanable: true,
                            });
                            id_counter += 1;
                            found = true;
                            logs.push(format!("  → {} · {}", name, format_bytes(sz)));
                        }
                    }
                }
                if !found {
                    logs.push("  ✓ Nothing to clean".to_string());
                }
                self.ivars().scan_progress.set(0.30);
                *self.ivars().scan_status_text.borrow_mut() = "Scanning application caches...".to_string();
            }
            2 => {
                logs.push("➤ Browsers".to_string());
                let mut found = false;
                for b in get_browser_cache_targets() {
                    let sz = calculate_dir_size(&b.path);
                    if sz > 0 {
                        buffer.push(ScanTreeItem {
                            id: id_counter,
                            category: "Browsers",
                            title: b.browser_name.to_string(),
                            detail: b.path.to_string_lossy().to_string(),
                            path: Some(b.path),
                            size_bytes: sz,
                            is_selected: true,
                            is_cleanable: true,
                        });
                        id_counter += 1;
                        found = true;
                        logs.push(format!("  → {} · {}", b.browser_name, format_bytes(sz)));
                    }
                }
                if !found {
                    logs.push("  ✓ Nothing to clean".to_string());
                }
                self.ivars().scan_progress.set(0.44);
                *self.ivars().scan_status_text.borrow_mut() = "Scanning browser profiles and web caches...".to_string();
            }
            3 => {
                logs.push("➤ Cloud & Office".to_string());
                let cloud_targets = [
                    ("Dropbox cache", dirs_home_path(".dropbox/cache")),
                    ("OneDrive cache", dirs_home_path("Library/Caches/OneDrive")),
                    ("Google Drive cache", dirs_home_path("Library/Application Support/Google/DriveFS")),
                    ("Microsoft Teams cache", dirs_home_path("Library/Caches/com.microsoft.teams2")),
                ];
                let mut found = false;
                for (name, path) in cloud_targets {
                    if path.exists() {
                        let sz = calculate_dir_size(&path);
                        if sz > 0 {
                            buffer.push(ScanTreeItem {
                                id: id_counter,
                                category: "Cloud & Office",
                                title: name.to_string(),
                                detail: path.to_string_lossy().to_string(),
                                path: Some(path),
                                size_bytes: sz,
                                is_selected: true,
                                is_cleanable: true,
                            });
                            id_counter += 1;
                            found = true;
                            logs.push(format!("  → {} · {}", name, format_bytes(sz)));
                        }
                    }
                }
                if !found {
                    logs.push("  ✓ Nothing to clean".to_string());
                }
                self.ivars().scan_progress.set(0.56);
                *self.ivars().scan_status_text.borrow_mut() = "Scanning cloud storage and office tools...".to_string();
            }
            4 => {
                logs.push("➤ Developer tools".to_string());
                let mut found = false;
                for dev in get_dev_cache_targets() {
                    let sz = calculate_dir_size(&dev.path);
                    if sz > 0 {
                        buffer.push(ScanTreeItem {
                            id: id_counter,
                            category: "Developer tools",
                            title: dev.name.to_string(),
                            detail: dev.path.to_string_lossy().to_string(),
                            path: Some(dev.path),
                            size_bytes: sz,
                            is_selected: true,
                            is_cleanable: true,
                        });
                        id_counter += 1;
                        found = true;
                        logs.push(format!("  → {} · {}", dev.name, format_bytes(sz)));
                    }
                }
                if !found {
                    logs.push("  ✓ Nothing to clean".to_string());
                }
                self.ivars().scan_progress.set(0.70);
                *self.ivars().scan_status_text.borrow_mut() = "Scanning developer tools (npm, Rust, Python, Xcode)...".to_string();
            }
            5 => {
                logs.push("➤ Apps & utilities".to_string());
                let mut found = false;
                for app in get_app_specific_caches() {
                    let sz = calculate_dir_size(&app.path);
                    if sz > 0 {
                        buffer.push(ScanTreeItem {
                            id: id_counter,
                            category: "Apps & utilities",
                            title: app.app_name.to_string(),
                            detail: app.path.to_string_lossy().to_string(),
                            path: Some(app.path),
                            size_bytes: sz,
                            is_selected: true,
                            is_cleanable: true,
                        });
                        id_counter += 1;
                        found = true;
                        logs.push(format!("  → {} · {}", app.app_name, format_bytes(sz)));
                    }
                }
                if !found {
                    logs.push("  ✓ Nothing to clean".to_string());
                }
                self.ivars().scan_progress.set(0.82);
                *self.ivars().scan_status_text.borrow_mut() = "Scanning desktop applications & utilities...".to_string();
            }
            6 => {
                logs.push("➤ Virtualization".to_string());
                let virt_targets = [
                    ("Docker container cache", dirs_home_path("Library/Containers/com.docker.docker/Data/log")),
                    ("OrbStack cache", dirs_home_path(".orbstack/cache")),
                    ("Lima cache", dirs_home_path(".lima")),
                    ("Podman cache", dirs_home_path(".local/share/containers")),
                ];
                let mut found_v = false;
                for (name, path) in virt_targets {
                    if path.exists() {
                        let sz = calculate_dir_size(&path);
                        if sz > 0 {
                            buffer.push(ScanTreeItem {
                                id: id_counter,
                                category: "Virtualization",
                                title: name.to_string(),
                                detail: path.to_string_lossy().to_string(),
                                path: Some(path),
                                size_bytes: sz,
                                is_selected: true,
                                is_cleanable: true,
                            });
                            id_counter += 1;
                            found_v = true;
                            logs.push(format!("  → {} · {}", name, format_bytes(sz)));
                        }
                    }
                }
                if !found_v {
                    logs.push("  ✓ Nothing to clean".to_string());
                }

                logs.push("➤ Application Support".to_string());
                let app_support_logs = dirs_home_path("Library/Application Support/logs");
                if app_support_logs.exists() {
                    let sz = calculate_dir_size(&app_support_logs);
                    if sz > 0 {
                        buffer.push(ScanTreeItem {
                            id: id_counter,
                            category: "Application Support",
                            title: "Application Support logs/caches".to_string(),
                            detail: app_support_logs.to_string_lossy().to_string(),
                            path: Some(app_support_logs),
                            size_bytes: sz,
                            is_selected: true,
                            is_cleanable: true,
                        });
                        logs.push(format!("  → Application Support logs · {}", format_bytes(sz)));
                    } else {
                        logs.push("  ✓ Nothing to clean".to_string());
                    }
                } else {
                    logs.push("  ✓ Nothing to clean".to_string());
                }

                self.ivars().scan_progress.set(0.92);
                *self.ivars().scan_status_text.borrow_mut() = "Scanning virtualization & leftovers...".to_string();
            }
            7 => {
                logs.push("➤ Project artifacts".to_string());
                let config = CleanerConfig::new();
                let purge_paths = config.load_purge_paths();
                let artifacts = scan_project_artifacts(&purge_paths);
                let mut found_art = false;
                for art in artifacts {
                    buffer.push(ScanTreeItem {
                        id: id_counter,
                        category: "Project artifacts",
                        title: format!("{} ({})", art.project_name, art.artifact_type),
                        detail: art.path.to_string_lossy().to_string(),
                        path: Some(art.path),
                        size_bytes: art.size_bytes,
                        is_selected: !art.is_recent,
                        is_cleanable: true,
                    });
                    id_counter += 1;
                    found_art = true;
                    logs.push(format!("  → {} ({}) · {}", art.project_name, art.artifact_type, format_bytes(art.size_bytes)));
                }
                if !found_art {
                    logs.push("  ✓ Nothing to clean".to_string());
                }

                logs.push("➤ Raw Installers".to_string());
                let installers = scan_installer_files();
                let mut found_inst = false;
                for inst in installers {
                    buffer.push(ScanTreeItem {
                        id: id_counter,
                        category: "Raw Installers",
                        title: inst.file_name.clone(),
                        detail: format!("{} — {}", inst.source_category, inst.path.display()),
                        path: Some(inst.path),
                        size_bytes: inst.size_bytes,
                        is_selected: true,
                        is_cleanable: true,
                    });
                    id_counter += 1;
                    found_inst = true;
                    logs.push(format!("  → {} · {}", inst.file_name, format_bytes(inst.size_bytes)));
                }
                if !found_inst {
                    logs.push("  ✓ Nothing to clean".to_string());
                }

                logs.push("➤ System".to_string());
                let mut found_sys = false;
                for sys in get_system_clean_targets() {
                    for path in &sys.paths {
                        let sz = calculate_dir_size(path);
                        if sz > 0 {
                            buffer.push(ScanTreeItem {
                                id: id_counter,
                                category: "System",
                                title: sys.name.to_string(),
                                detail: path.to_string_lossy().to_string(),
                                path: Some(path.clone()),
                                size_bytes: sz,
                                is_selected: true,
                                is_cleanable: true,
                            });
                            id_counter += 1;
                            found_sys = true;
                            logs.push(format!("  → {} · {}", sys.name, format_bytes(sz)));
                        }
                    }
                }
                if !found_sys {
                    logs.push("  ✓ Nothing to clean".to_string());
                }

                self.ivars().scan_progress.set(1.0);
                *self.ivars().scan_status_text.borrow_mut() = "Finalizing scan report...".to_string();
            }
            _ => {
                let total_bytes: u64 = buffer.iter().map(|i| i.size_bytes).sum();
                let total_items = buffer.len();
                logs.push("================================================================================──────────".to_string());
                logs.push("Scan complete - Ready for cleanup".to_string());
                logs.push(format!("Potential space: {} | Items: {}", format_bytes(total_bytes), total_items));
                logs.push("================================================================================──────────".to_string());

                *self.ivars().items.borrow_mut() = buffer.clone();
                self.ivars().is_scanning.store(false, Ordering::SeqCst);
                self.ivars().scan_progress.set(1.0);
                *self.ivars().scan_status_text.borrow_mut() = "Scan complete (100%)".to_string();

                drop(logs);
                drop(buffer);

                self.render_main_content();
                return;
            }
        }

        self.ivars().scan_step.set(step + 1);
        drop(logs);
        drop(buffer);

        self.render_main_content();
        self.schedule_scan_step_timer(0.08);
    }

    fn handle_clean_confirmation(&self) {
        let mtm = self.mtm();
        let current_tab = *self.ivars().current_tab.borrow();
        let items = self.ivars().items.borrow();

        let filter_cat = filter_category_for_tab(current_tab);

        let selected_count = items
            .iter()
            .filter(|i| filter_cat.is_none() || Some(i.category) == filter_cat)
            .filter(|i| i.is_selected)
            .count();

        let selected_bytes: u64 = items
            .iter()
            .filter(|i| filter_cat.is_none() || Some(i.category) == filter_cat)
            .filter(|i| i.is_selected)
            .map(|i| i.size_bytes)
            .sum();
        drop(items);

        if selected_count == 0 {
            return;
        }

        let is_dry = self.ivars().is_dry_run.get();
        if is_dry {
            let alert = NSAlert::new(mtm);
            alert.setMessageText(ns_string!("Dry Run Simulation Verified"));
            alert.setInformativeText(&NSString::from_str(&format!(
                "Dry run completed successfully for {} items.\n\nEstimated reclaimable space: {}\n\n(No files or directories were deleted on disk)",
                selected_count,
                format_bytes(selected_bytes)
            )));
            alert.addButtonWithTitle(ns_string!("OK"));
            alert.runModal();
            return;
        }

        let alert = NSAlert::new(mtm);
        alert.setAlertStyle(NSAlertStyle::Warning);
        alert.setMessageText(&NSString::from_str("Confirm Permanent Cleanup"));
        alert.setInformativeText(&NSString::from_str(&format!(
            "Are you sure you want to clean {} selected items?\n\nThis will reclaim approx {} of disk space. Cache files, build artifacts, and selected installers will be permanently removed or moved to Trash.",
            selected_count,
            format_bytes(selected_bytes)
        )));
        alert.addButtonWithTitle(ns_string!("Clean Now"));
        alert.addButtonWithTitle(ns_string!("Cancel"));

        let response = alert.runModal();
        if response == NSAlertFirstButtonReturn {
            self.execute_cleaning_pass(filter_cat);
        }
    }

    fn execute_cleaning_pass(&self, filter_category: Option<&str>) {
        let mtm = self.mtm();
        let config = CleanerConfig::new();
        let whitelist = config.load_whitelist();
        let validator = PathValidator::new(whitelist);
        let logger = HistoryLogger::new();

        let mut items = self.ivars().items.borrow_mut();
        let mut total_freed = 0u64;
        let mut deleted_ids = Vec::new();

        for item in items.iter_mut() {
            if filter_category.is_none() || Some(item.category) == filter_category {
                if item.is_selected {
                    if let Some(ref path) = item.path {
                        if validator.is_safe_to_delete(path).is_ok() {
                            let res = robust_delete_path(path);
                            if res.is_ok() {
                                total_freed += item.size_bytes;
                                logger.log_deletion(path, item.size_bytes, true);
                                deleted_ids.push(item.id);
                            }
                        }
                    }
                }
            }
        }

        items.retain(|i| !deleted_ids.contains(&i.id));

        logger.log_operation("gui_clean", "selected_items", total_freed, "success");
        drop(items);

        let success_alert = NSAlert::new(mtm);
        success_alert.setMessageText(ns_string!("Cleanup Completed"));
        success_alert.setInformativeText(&NSString::from_str(&format!(
            "Successfully cleaned selected items and freed {} of disk space.",
            format_bytes(total_freed)
        )));
        success_alert.addButtonWithTitle(ns_string!("OK"));
        success_alert.runModal();

        self.render_main_content();
    }
}

fn robust_delete_path(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if !path.exists() {
        return Ok(());
    }

    if path.is_file() || path.is_symlink() {
        if std::fs::remove_file(path).is_err() {
            if let Ok(meta) = std::fs::metadata(path) {
                let mut perms = meta.permissions();
                perms.set_mode(perms.mode() | 0o777);
                let _ = std::fs::set_permissions(path, perms);
            }
            std::fs::remove_file(path)?;
        }
        return Ok(());
    }

    if path.is_dir() {
        if std::fs::remove_dir_all(path).is_ok() {
            return Ok(());
        }

        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o777);
            let _ = std::fs::set_permissions(path, perms);
        }

        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let p = entry.path();
                let _ = robust_delete_path(&p);
            }
        }
        let _ = std::fs::remove_dir(path);
    }
    Ok(())
}

fn filter_category_for_tab(tab: TabIndex) -> Option<&'static str> {
    match tab {
        TabIndex::ProjectPurge => Some("Project artifacts"),
        TabIndex::InstallerClean => Some("Raw Installers"),
        TabIndex::DeepClean => None,
        _ => None,
    }
}

fn dirs_home_path(sub: &str) -> PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.home_dir().join(sub))
        .unwrap_or_else(|| PathBuf::from("/").join(sub))
}
