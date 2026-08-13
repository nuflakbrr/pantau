//! Low-level Mach host/processor FFI bindings not exposed by the `mach2`
//! crate (0.4.3 has no `host_processor_info`/`host_statistics64`). Hand-rolled
//! against public `mach/mach_host.h`, `mach/vm_statistics.h`,
//! `mach/processor_info.h` — shared by `cpu.rs` and `memory.rs`.
#![allow(non_camel_case_types)]

use mach2::kern_return::{kern_return_t, KERN_SUCCESS};
use mach2::mach_types::host_t;
use mach2::port::mach_port_t;
use mach2::traps::mach_task_self;

pub type natural_t = u32;
pub type integer_t = i32;
pub type mach_msg_type_number_t = natural_t;

pub const HOST_VM_INFO64: i32 = 4;
pub const PROCESSOR_CPU_LOAD_INFO: i32 = 2;
pub const CPU_STATE_MAX: usize = 4;
pub const CPU_STATE_USER: usize = 0;
pub const CPU_STATE_SYSTEM: usize = 1;
pub const CPU_STATE_IDLE: usize = 2;
pub const CPU_STATE_NICE: usize = 3;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct vm_statistics64 {
    pub free_count: natural_t,
    pub active_count: natural_t,
    pub inactive_count: natural_t,
    pub wire_count: natural_t,
    pub zero_fill_count: u64,
    pub reactivations: u64,
    pub pageins: u64,
    pub pageouts: u64,
    pub faults: u64,
    pub cow_faults: u64,
    pub lookups: u64,
    pub hits: u64,
    pub purges: u64,
    pub purgeable_count: natural_t,
    pub speculative_count: natural_t,
    pub decompressions: u64,
    pub compressions: u64,
    pub swapins: u64,
    pub swapouts: u64,
    pub compressor_page_count: natural_t,
    pub throttled_count: natural_t,
    pub external_page_count: natural_t,
    pub internal_page_count: natural_t,
    pub total_uncompressed_pages_in_compressor: u64,
}

unsafe extern "C" {
    fn mach_host_self() -> mach_port_t;

    fn host_statistics64(
        host_priv: host_t,
        flavor: i32,
        host_info_out: *mut i32,
        host_info_out_cnt: *mut mach_msg_type_number_t,
    ) -> kern_return_t;

    fn host_processor_info(
        host: host_t,
        flavor: i32,
        out_processor_count: *mut natural_t,
        out_processor_info: *mut *mut integer_t,
        out_processor_info_cnt: *mut mach_msg_type_number_t,
    ) -> kern_return_t;

    fn vm_deallocate(target_task: mach_port_t, address: usize, size: usize) -> kern_return_t;
}

pub fn read_vm_statistics64() -> Option<vm_statistics64> {
    unsafe {
        let host = mach_host_self();
        let mut stats = vm_statistics64::default();
        let mut count = (std::mem::size_of::<vm_statistics64>() / std::mem::size_of::<integer_t>())
            as mach_msg_type_number_t;
        let kr = host_statistics64(host, HOST_VM_INFO64, &mut stats as *mut _ as *mut i32, &mut count);
        if kr == KERN_SUCCESS {
            Some(stats)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CpuTicks {
    pub user: u32,
    pub system: u32,
    pub idle: u32,
    pub nice: u32,
}

impl CpuTicks {
    pub fn total(&self) -> u64 {
        self.user as u64 + self.system as u64 + self.idle as u64 + self.nice as u64
    }

    pub fn busy(&self) -> u64 {
        self.user as u64 + self.system as u64 + self.nice as u64
    }
}

/// One reading per logical core, tick counters are cumulative since boot.
pub fn read_processor_cpu_load() -> Option<Vec<CpuTicks>> {
    unsafe {
        let host = mach_host_self();
        let mut num_cpus: natural_t = 0;
        let mut info: *mut integer_t = std::ptr::null_mut();
        let mut info_cnt: mach_msg_type_number_t = 0;
        let kr = host_processor_info(
            host,
            PROCESSOR_CPU_LOAD_INFO,
            &mut num_cpus,
            &mut info,
            &mut info_cnt,
        );
        if kr != KERN_SUCCESS || info.is_null() {
            return None;
        }
        let slice = std::slice::from_raw_parts(info as *const u32, num_cpus as usize * CPU_STATE_MAX);
        let mut loads = Vec::with_capacity(num_cpus as usize);
        for i in 0..num_cpus as usize {
            let base = i * CPU_STATE_MAX;
            loads.push(CpuTicks {
                user: slice[base + CPU_STATE_USER],
                system: slice[base + CPU_STATE_SYSTEM],
                idle: slice[base + CPU_STATE_IDLE],
                nice: slice[base + CPU_STATE_NICE],
            });
        }
        let dealloc_size = info_cnt as usize * std::mem::size_of::<integer_t>();
        vm_deallocate(mach_task_self(), info as usize, dealloc_size);
        Some(loads)
    }
}

/// `sysctlbyname` string read (e.g. `machdep.cpu.brand_string`, `kern.osproductversion`).
pub fn sysctl_string(name: &str) -> Option<String> {
    use std::ffi::CString;
    let cname = CString::new(name).ok()?;
    unsafe {
        let mut size: libc::size_t = 0;
        if libc::sysctlbyname(cname.as_ptr(), std::ptr::null_mut(), &mut size, std::ptr::null_mut(), 0) != 0 {
            return None;
        }
        let mut buf = vec![0u8; size];
        if libc::sysctlbyname(
            cname.as_ptr(),
            buf.as_mut_ptr() as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        buf.truncate(size.saturating_sub(1));
        String::from_utf8(buf).ok()
    }
}

/// `sysctlbyname` integer read (e.g. `hw.physicalcpu`, `hw.cpufrequency`).
pub fn sysctl_u64(name: &str) -> Option<u64> {
    use std::ffi::CString;
    let cname = CString::new(name).ok()?;
    unsafe {
        let mut value: u64 = 0;
        let mut size = std::mem::size_of::<u64>() as libc::size_t;
        if libc::sysctlbyname(
            cname.as_ptr(),
            &mut value as *mut _ as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        ) == 0
        {
            if size == std::mem::size_of::<u32>() {
                return Some((value as u32) as u64);
            }
            return Some(value);
        }
        None
    }
}
