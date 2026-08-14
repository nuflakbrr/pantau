use super::sys;

fn page_size_bytes() -> u64 {
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as u64 }
}

// allocated_bytes is a real computed value with no display consumer yet
// (the menu uses usage_percent) — kept rather than deleted.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct MemoryReading {
    pub physical_bytes: Option<u64>,
    pub allocated_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub usage_percent: Option<f64>,
    /// Linux page-cache has no direct macOS equivalent (unified VM, no
    /// separate page-cache concept). Approximated via file-backed
    /// (external) pages — noted as approximate, not identical, per plan.
    pub cached_approx_bytes: Option<u64>,
}

pub fn read_memory() -> MemoryReading {
    let stats = match sys::read_vm_statistics64() {
        Some(s) => s,
        None => return MemoryReading::default(),
    };
    let page = page_size_bytes();
    let physical_bytes = sys::sysctl_u64("hw.memsize");

    let allocated_bytes =
        (stats.active_count as u64 + stats.wire_count as u64 + stats.compressor_page_count as u64) * page;
    let free_bytes = stats.free_count as u64 * page;
    let available_bytes = free_bytes + stats.inactive_count as u64 * page;
    let cached_approx_bytes = stats.external_page_count as u64 * page;

    let usage_percent = physical_bytes
        .filter(|&p| p > 0)
        .map(|p| allocated_bytes as f64 / p as f64 * 100.0);

    MemoryReading {
        physical_bytes,
        allocated_bytes: Some(allocated_bytes),
        available_bytes: Some(available_bytes),
        free_bytes: Some(free_bytes),
        usage_percent,
        cached_approx_bytes: Some(cached_approx_bytes),
    }
}

/// Mirrors `struct xsw_usage` from `sys/sysctl.h`, read via
/// `sysctlbyname("vm.swapusage")`.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct XswUsage {
    total: u64,
    avail: u64,
    used: u64,
    pagesize: u32,
    encrypted: i32,
}

// free_bytes is a real computed value with no display consumer yet (the
// menu uses usage_percent) — kept rather than deleted.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct SwapReading {
    pub total_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub used_bytes: Option<u64>,
    pub usage_percent: Option<f64>,
}

pub fn read_swap() -> SwapReading {
    use std::ffi::CString;
    let name = match CString::new("vm.swapusage") {
        Ok(n) => n,
        Err(_) => return SwapReading::default(),
    };
    let mut usage = XswUsage::default();
    let mut size = std::mem::size_of::<XswUsage>() as libc::size_t;
    let ok = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            &mut usage as *mut _ as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        ) == 0
    };
    if !ok {
        return SwapReading::default();
    }
    let usage_percent = if usage.total > 0 {
        Some(usage.used as f64 / usage.total as f64 * 100.0)
    } else {
        None
    };
    SwapReading {
        total_bytes: Some(usage.total),
        free_bytes: Some(usage.avail),
        used_bytes: Some(usage.used),
        usage_percent,
    }
}
