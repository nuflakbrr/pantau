//! Apple SMC bridge via the `AppleSMC` IOKit user client. `SMCParamStruct`
//! layout below is the widely-replicated 80-byte struct originally
//! documented by smcFanControl and reused across dozens of open-source SMC
//! readers (Rust reimplementation here, not a source transcript). A
//! compile-time size assert catches any field-layout mistake immediately —
//! no need to guess at runtime.
use core_foundation_sys::dictionary::CFMutableDictionaryRef;
use mach2::kern_return::{kern_return_t, KERN_SUCCESS};
use mach2::port::mach_port_t;
use mach2::traps::mach_task_self;
use std::ffi::{c_char, c_void, CString};

type IoServiceT = mach_port_t;
type IoConnectT = mach_port_t;

const KIO_MASTER_PORT_DEFAULT: mach_port_t = 0;
const K_SMC_HANDLE_YPC_EVENT: u32 = 2;
const K_SMC_READ_KEY: u8 = 5;
const K_SMC_GET_KEY_FROM_INDEX: u8 = 8;
const K_SMC_GET_KEY_INFO: u8 = 9;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct SmcVersion {
    major: u8,
    minor: u8,
    build: u8,
    reserved: u8,
    release: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct SmcPLimitData {
    version: u16,
    length: u16,
    cpu_p_limit: u32,
    gpu_p_limit: u32,
    mem_p_limit: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct SmcKeyInfoData {
    data_size: u32,
    data_type: u32,
    data_attributes: u8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct SmcParamStruct {
    key: u32,
    vers: SmcVersion,
    p_limit_data: SmcPLimitData,
    key_info: SmcKeyInfoData,
    result: u8,
    status: u8,
    data8: u8,
    data32: u32,
    bytes: [u8; 32],
}

const _: () = assert!(std::mem::size_of::<SmcParamStruct>() == 80);

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CFMutableDictionaryRef;
    fn IOServiceGetMatchingService(master_port: mach_port_t, matching: CFMutableDictionaryRef) -> IoServiceT;
    fn IOServiceOpen(service: IoServiceT, owning_task: mach_port_t, ty: u32, connect: *mut IoConnectT) -> kern_return_t;
    fn IOServiceClose(connect: IoConnectT) -> kern_return_t;
    fn IOObjectRelease(object: mach_port_t) -> kern_return_t;
    fn IOConnectCallStructMethod(
        connection: IoConnectT,
        selector: u32,
        input_struct: *const c_void,
        input_struct_cnt: usize,
        output_struct: *mut c_void,
        output_struct_cnt: *mut usize,
    ) -> kern_return_t;
}

/// FourCC key code, e.g. `"TC0P"` -> big-endian packed `u32`.
fn key_code(key: &str) -> u32 {
    let bytes = key.as_bytes();
    let mut padded = [b' '; 4];
    for (i, b) in bytes.iter().take(4).enumerate() {
        padded[i] = *b;
    }
    u32::from_be_bytes(padded)
}

fn fourcc_to_string(code: u32) -> String {
    String::from_utf8_lossy(&code.to_be_bytes()).trim_end().to_string()
}

pub struct SmcConnection {
    connect: IoConnectT,
}

impl SmcConnection {
    pub fn open() -> Option<Self> {
        unsafe {
            let name = CString::new("AppleSMC").ok()?;
            let matching = IOServiceMatching(name.as_ptr());
            if matching.is_null() {
                return None;
            }
            // IOServiceGetMatchingService consumes the matching dictionary
            // reference — no CFRelease needed on `matching` itself.
            let service = IOServiceGetMatchingService(KIO_MASTER_PORT_DEFAULT, matching);
            if service == 0 {
                return None;
            }
            let mut connect: IoConnectT = 0;
            let kr = IOServiceOpen(service, mach_task_self(), 0, &mut connect);
            IOObjectRelease(service);
            if kr != KERN_SUCCESS {
                return None;
            }
            Some(Self { connect })
        }
    }

    fn call(&self, input: &SmcParamStruct) -> Option<SmcParamStruct> {
        let mut output = SmcParamStruct::default();
        let mut out_size = std::mem::size_of::<SmcParamStruct>();
        let kr = unsafe {
            IOConnectCallStructMethod(
                self.connect,
                K_SMC_HANDLE_YPC_EVENT,
                input as *const _ as *const c_void,
                std::mem::size_of::<SmcParamStruct>(),
                &mut output as *mut _ as *mut c_void,
                &mut out_size,
            )
        };
        if kr != KERN_SUCCESS || output.result != 0 {
            return None;
        }
        Some(output)
    }

    /// Returns `None` gracefully if the key doesn't exist on this Mac
    /// (fanless model, no discrete GPU, etc.) — never panics.
    pub fn read_key(&self, key: &str) -> Option<f64> {
        let code = key_code(key);

        let mut info_input = SmcParamStruct::default();
        info_input.key = code;
        info_input.data8 = K_SMC_GET_KEY_INFO;
        let info_output = self.call(&info_input)?;
        let data_size = info_output.key_info.data_size as usize;
        if data_size == 0 || data_size > 32 {
            return None;
        }
        let data_type = fourcc_to_string(info_output.key_info.data_type);

        let mut read_input = SmcParamStruct::default();
        read_input.key = code;
        read_input.key_info.data_size = info_output.key_info.data_size;
        read_input.data8 = K_SMC_READ_KEY;
        let read_output = self.call(&read_input)?;
        let bytes = &read_output.bytes[..data_size];

        decode(&data_type, bytes)
    }

    /// Startup-only enumeration helper — walks the SMC's flat key table by
    /// index. Not meant to be called every poll tick (a few thousand IOKit
    /// round-trips), only once by [`ThermalProbe::discover`].
    pub fn key_name_at_index(&self, index: u32) -> Option<String> {
        let mut input = SmcParamStruct::default();
        input.data8 = K_SMC_GET_KEY_FROM_INDEX;
        input.data32 = index;
        let output = self.call(&input)?;
        Some(fourcc_to_string(output.key))
    }

    pub fn key_type_of(&self, key: &str) -> Option<String> {
        let mut input = SmcParamStruct::default();
        input.key = key_code(key);
        input.data8 = K_SMC_GET_KEY_INFO;
        let output = self.call(&input)?;
        Some(fourcc_to_string(output.key_info.data_type))
    }

    pub fn key_exists(&self, key: &str) -> bool {
        let code = key_code(key);
        let mut input = SmcParamStruct::default();
        input.key = code;
        input.data8 = K_SMC_GET_KEY_INFO;
        self.call(&input).is_some()
    }
}

impl Drop for SmcConnection {
    fn drop(&mut self) {
        unsafe {
            IOServiceClose(self.connect);
        }
    }
}

fn decode(data_type: &str, bytes: &[u8]) -> Option<f64> {
    match data_type {
        "sp78" if bytes.len() >= 2 => {
            let raw = i16::from_be_bytes([bytes[0], bytes[1]]);
            Some(raw as f64 / 256.0)
        }
        "flt" if bytes.len() >= 4 => {
            let raw = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            Some(raw as f64)
        }
        "ui8" if !bytes.is_empty() => Some(bytes[0] as f64),
        "ui16" if bytes.len() >= 2 => Some(u16::from_be_bytes([bytes[0], bytes[1]]) as f64),
        "ui32" if bytes.len() >= 4 => {
            Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_layout_is_80_bytes() {
        assert_eq!(std::mem::size_of::<SmcParamStruct>(), 80);
    }

    // Real-hardware smoke test, not run by default (`cargo test -- --ignored`)
    // — validates the FFI actually round-trips on real SMC hardware, not
    // just that it compiles.
    #[test]
    #[ignore]
    fn reads_key_count() {
        let conn = SmcConnection::open().expect("AppleSMC should be openable");
        let value = conn.read_key("#KEY").expect("#KEY should exist on every Mac");
        let count = value;
        println!("#KEY = {count}");
        assert!(count > 100.0, "implausible SMC key count: {count}");
    }

    // Real key names confirmed by hand on Apple Silicon (arm64) during
    // development: Tp0* is the P-core diode cluster (TC0P/TC0D are the
    // Intel-only equivalents), F0Ac/F1Ac + FNum/F0Mn/F0Mx match the plan's
    // documented fan key set exactly, VC1x-VC3x is the core voltage
    // cluster. Kept as an ignored smoke test, not a hard dependency —
    // different Apple Silicon generations may expose a different subset.
    #[test]
    #[ignore]
    fn reads_plausible_cpu_temperature() {
        let conn = SmcConnection::open().expect("AppleSMC should be openable");
        let celsius = conn.read_key("Tp01").or_else(|| conn.read_key("TC0P"));
        let celsius = celsius.expect("Tp01 (Apple Silicon) or TC0P (Intel) should exist");
        assert!(celsius > 0.0 && celsius < 130.0, "implausible CPU temp: {celsius}");
    }
}
