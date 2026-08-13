//! GPU utilization/static info. Apple Silicon integrated GPUs have no
//! public utilization API — confirmed here, not assumed: real-hardware
//! testing during development found no `hw.perflevel0.gpuclockmax`-family
//! sysctl (`sysctl -a | grep gpu` on this arm64 Mac shows only
//! `iogpu.*`/`debug.iogpu.*` memory-limit knobs, nothing usable for
//! utilization or core count). GPU core count lives in the IORegistry
//! `AGXAccelerator` node's `gpu-core-count` property instead, which needs
//! the same CFDictionary-registry-walk machinery already deferred for disk
//! I/O throughput (`disk.rs`) — same risk class, same reason for deferring:
//! no second Mac model available this session to cross-check the parse
//! against, so it stays unimplemented rather than shipped unverified.
//!
//! Intel Mac + AMD discrete GPU utilization (`IOAccelerator` registry
//! `PerformanceStatistics` dict) is real per public documentation but this
//! dev machine is Apple Silicon-only — no hardware to test against, so it's
//! deliberately not implemented rather than guessed.
//!
//! NVIDIA (`nvidia-smi` subprocess in vitals-gnome) is dropped entirely —
//! no NVIDIA driver support on any current macOS.

#[derive(Debug, Clone, Default)]
pub struct GpuReading {
    /// `None` on Apple Silicon (no public API) — always `None` on every Mac
    /// this app has been tested on so far, not a probe failure.
    pub utilization_percent: Option<f64>,
    pub vram_used_bytes: Option<u64>,
    pub vram_total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct GpuStaticInfo {
    pub model_name: Option<String>,
    pub core_count: Option<u32>,
    pub vram_total_bytes: Option<u64>,
}

pub fn read_utilization() -> GpuReading {
    GpuReading::default()
}

pub fn read_static_info() -> GpuStaticInfo {
    GpuStaticInfo::default()
}
