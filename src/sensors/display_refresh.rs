//! Display refresh rate. `CGDisplayModeGetRefreshRate` is the documented
//! API but returns `0.0` on many modern built-in displays (ProMotion /
//! variable-refresh panels don't report a fixed nominal rate through the
//! legacy CoreGraphics path) — confirmed on this dev Mac's built-in display.
//! Falls back to `CVDisplayLinkGetActualOutputVideoRefreshPeriod` (1/period)
//! from CoreVideo, which does report a real number in that case.
use std::os::raw::c_void;

type CgDirectDisplayId = u32;
type CgDisplayModeRef = *mut c_void;
type CvDisplayLinkRef = *mut c_void;
type CvReturn = i32;

const KCV_RETURN_SUCCESS: CvReturn = 0;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGMainDisplayID() -> CgDirectDisplayId;
    fn CGDisplayCopyDisplayMode(display: CgDirectDisplayId) -> CgDisplayModeRef;
    fn CGDisplayModeGetRefreshRate(mode: CgDisplayModeRef) -> f64;
    fn CGDisplayModeRelease(mode: CgDisplayModeRef);
}

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    fn CVDisplayLinkCreateWithCGDisplay(display_id: CgDirectDisplayId, link_out: *mut CvDisplayLinkRef) -> CvReturn;
    fn CVDisplayLinkGetActualOutputVideoRefreshPeriod(link: CvDisplayLinkRef) -> f64;
    fn CVDisplayLinkRelease(link: CvDisplayLinkRef);
}

fn read_via_display_mode(display_id: CgDirectDisplayId) -> Option<f64> {
    unsafe {
        let mode = CGDisplayCopyDisplayMode(display_id);
        if mode.is_null() {
            return None;
        }
        let hz = CGDisplayModeGetRefreshRate(mode);
        CGDisplayModeRelease(mode);
        if hz > 0.0 {
            Some(hz)
        } else {
            None
        }
    }
}

fn read_via_display_link(display_id: CgDirectDisplayId) -> Option<f64> {
    unsafe {
        let mut link: CvDisplayLinkRef = std::ptr::null_mut();
        let kr = CVDisplayLinkCreateWithCGDisplay(display_id, &mut link);
        if kr != KCV_RETURN_SUCCESS || link.is_null() {
            return None;
        }
        let period = CVDisplayLinkGetActualOutputVideoRefreshPeriod(link);
        CVDisplayLinkRelease(link);
        if period > 0.0 {
            Some(1.0 / period)
        } else {
            None
        }
    }
}

/// Main display's refresh rate in Hz. `None` only if both APIs fail to
/// produce a usable number — graceful degradation, same contract as every
/// other sensor in this codebase.
pub fn read_main_display_refresh_hz() -> Option<f64> {
    let display_id = unsafe { CGMainDisplayID() };
    read_via_display_mode(display_id).or_else(|| read_via_display_link(display_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn reads_plausible_refresh_rate() {
        let hz = read_main_display_refresh_hz().expect("main display should report a refresh rate");
        println!("refresh rate = {hz} Hz");
        assert!(hz >= 30.0 && hz <= 240.0, "implausible refresh rate: {hz}");
    }
}
