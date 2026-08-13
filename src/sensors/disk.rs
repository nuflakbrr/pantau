use std::ffi::CString;

#[derive(Debug, Clone, Default)]
pub struct DiskReading {
    pub total_bytes: Option<u64>,
    pub used_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub used_percent: Option<f64>,
    pub free_percent: Option<f64>,
    /// ext4/zfs root-reserved-space has no APFS equivalent (container-level
    /// free-space sharing, not per-volume reserved blocks) — always `None`,
    /// not a probe failure.
    pub reserved_bytes: Option<u64>,
}

pub fn read_usage(path: &str) -> DiskReading {
    let Ok(cpath) = CString::new(path) else {
        return DiskReading::default();
    };
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::statfs(cpath.as_ptr(), &mut stat) == 0 };
    if !ok {
        return DiskReading::default();
    }

    let block_size = stat.f_bsize as u64;
    let total_bytes = stat.f_blocks * block_size;
    let free_bytes = stat.f_bavail * block_size;
    // Both derive from f_bavail (blocks available to unprivileged users),
    // not f_bfree (raw free blocks incl. root-reserved space) — otherwise
    // used_bytes + free_bytes < total_bytes on filesystems that reserve
    // space (non-APFS volumes, exFAT/FAT externals).
    let used_bytes = total_bytes.saturating_sub(free_bytes);

    let used_percent = if total_bytes > 0 {
        Some(used_bytes as f64 / total_bytes as f64 * 100.0)
    } else {
        None
    };
    let free_percent = if total_bytes > 0 {
        Some(free_bytes as f64 / total_bytes as f64 * 100.0)
    } else {
        None
    };

    DiskReading {
        total_bytes: Some(total_bytes),
        used_bytes: Some(used_bytes),
        free_bytes: Some(free_bytes),
        used_percent,
        free_percent,
        reserved_bytes: None,
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DiskIoReading {
    pub read_bytes_total: Option<u64>,
    pub write_bytes_total: Option<u64>,
}

// ponytail: IOBlockStorageDriver read/write throughput needs walking the
// IOKit registry (IOServiceGetMatchingService + IORegistryEntryCreateCFProperties)
// and parsing a nested CFDictionary "Statistics" entry — real risk surface
// for a first pass with no interactive hardware iteration budget left this
// session. Returns None (graceful degradation, same contract as every other
// probe in this codebase) until that's implemented with `io-kit-sys` +
// `core-foundation`.
pub fn read_io() -> DiskIoReading {
    DiskIoReading::default()
}
