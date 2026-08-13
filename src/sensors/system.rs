use super::sys;
use std::ffi::CString;

#[derive(Debug, Clone, Default)]
pub struct SystemReading {
    pub load1: Option<f64>,
    pub load5: Option<f64>,
    pub load15: Option<f64>,
    pub uptime_seconds: Option<f64>,
    pub open_files: Option<u64>,
    /// Best-effort: `proc_pidinfo` fails silently (returns 0) for processes
    /// not owned by the current user, so this undercounts on a
    /// multi-user/sandboxed system rather than requiring root.
    pub threads_total: Option<u64>,
    pub processes_total: Option<u64>,
}

pub fn read_load_average() -> (Option<f64>, Option<f64>, Option<f64>) {
    let mut loads = [0.0f64; 3];
    let n = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
    if n == 3 {
        (Some(loads[0]), Some(loads[1]), Some(loads[2]))
    } else {
        (None, None, None)
    }
}

/// Mirrors `struct timeval` from `sys/time.h`, read via `sysctlbyname("kern.boottime")`.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct Timeval {
    tv_sec: i64,
    tv_usec: i32,
}

pub fn read_uptime_seconds() -> Option<f64> {
    let name = CString::new("kern.boottime").ok()?;
    let mut boottime = Timeval::default();
    let mut size = std::mem::size_of::<Timeval>() as libc::size_t;
    let ok = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            &mut boottime as *mut _ as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        ) == 0
    };
    if !ok || boottime.tv_sec == 0 {
        return None;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some((now.as_secs() as f64 - boottime.tv_sec as f64).max(0.0))
}

pub fn read_open_files() -> Option<u64> {
    sys::sysctl_u64("kern.num_files")
}

const PROC_PIDTASKINFO: i32 = 4;

/// Mirrors `struct proc_taskinfo` from `sys/proc_info.h`; only the leading
/// fields up to `pti_threadnum` are read.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct ProcTaskInfo {
    pti_virtual_size: u64,
    pti_resident_size: u64,
    pti_total_user: u64,
    pti_total_system: u64,
    pti_threads_user: u64,
    pti_threads_system: u64,
    pti_policy: i32,
    pti_faults: i32,
    pti_pageins: i32,
    pti_cow_faults: i32,
    pti_messages_sent: i32,
    pti_messages_received: i32,
    pti_syscalls_mach: i32,
    pti_syscalls_unix: i32,
    pti_csw: i32,
    pti_threadnum: i32,
    pti_numrunning: i32,
    pti_priority: i32,
}

unsafe extern "C" {
    fn proc_listallpids(buffer: *mut libc::c_void, buffersize: libc::c_int) -> libc::c_int;
    fn proc_pidinfo(
        pid: libc::c_int,
        flavor: libc::c_int,
        arg: u64,
        buffer: *mut libc::c_void,
        buffersize: libc::c_int,
    ) -> libc::c_int;
}

pub fn read_process_thread_totals() -> (Option<u64>, Option<u64>) {
    unsafe {
        let needed = proc_listallpids(std::ptr::null_mut(), 0);
        if needed <= 0 {
            return (None, None);
        }
        let mut pids = vec![0i32; needed as usize + 32];
        let bytes = proc_listallpids(
            pids.as_mut_ptr() as *mut libc::c_void,
            (pids.len() * std::mem::size_of::<i32>()) as libc::c_int,
        );
        if bytes <= 0 {
            return (None, None);
        }
        let count = bytes as usize / std::mem::size_of::<i32>();
        pids.truncate(count);

        let mut threads_total: u64 = 0;
        for &pid in &pids {
            if pid <= 0 {
                continue;
            }
            let mut info = ProcTaskInfo::default();
            let written = proc_pidinfo(
                pid,
                PROC_PIDTASKINFO,
                0,
                &mut info as *mut _ as *mut libc::c_void,
                std::mem::size_of::<ProcTaskInfo>() as libc::c_int,
            );
            if written as usize == std::mem::size_of::<ProcTaskInfo>() {
                threads_total += info.pti_threadnum.max(0) as u64;
            }
        }
        (Some(threads_total), Some(count as u64))
    }
}

pub fn read_system() -> SystemReading {
    let (load1, load5, load15) = read_load_average();
    let (threads_total, processes_total) = read_process_thread_totals();
    SystemReading {
        load1,
        load5,
        load15,
        uptime_seconds: read_uptime_seconds(),
        open_files: read_open_files(),
        threads_total,
        processes_total,
    }
}
