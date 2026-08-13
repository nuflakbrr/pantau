# Pantau

Menu bar system monitor native untuk macOS, ditulis dalam Rust. Menampilkan CPU, memory, swap, storage, network, WiFi, suhu/fan (via SMC), voltase, refresh rate display, battery, GPU, dan public IP langsung dari status bar — tanpa Electron, tanpa binary bundel besar.

Terinspirasi dari GNOME Shell extension [vitals](https://github.com/corecoding/Vitals), diporting ke arsitektur native macOS (AppKit via `objc2`).

## Fitur

- **Status bar item** dengan ikon SF Symbol per sensor, judul bisa di-pin ke beberapa metrik sekaligus (mode ringkas `[icon] value`).
- **Menu accordion** per kategori: CPU, Memory, System, Storage, Network, Battery, GPU — detail baris di-update di tempat (tidak rebuild total) supaya submenu yang sedang terbuka tidak berkedip.
- **Sensor yang didukung**:
  - CPU: total usage, per-core usage, frekuensi, waktu proses, suhu (rata-rata via SMC)
  - Memory: physical, available, free, cached (approx)
  - Swap: usage percent, used/total
  - Storage: usage percent root volume (`/`), used/total
  - System: uptime, load average (1/5/15), open files, threads, jumlah proses
  - Network: throughput up/down agregat, per-interface detail
  - WiFi: link quality, RSSI, noise (dBm) via CoreWLAN
  - Thermal: suhu CPU, fan RPM (hingga 4 fan), voltase rata-rata — via Apple SMC
  - Display: refresh rate (Hz) monitor utama
  - Battery: persentase health, cycle count, voltase, rate pengisian/pemakaian, estimasi waktu tersisa (interpolasi per-detik)
  - GPU: utilization percent, nama model (jika tersedia API publik)
  - Public IP: opsional, dengan flag negara, beberapa provider (Ipify / MyIP / CoreCoding), interval polling custom
- **Preferensi tersimpan** ke `~/Library/Application Support/pantau-app/config.toml` (TOML) — gagal baca/parse otomatis fallback ke default, tidak pernah crash saat startup.
- **Row tetap terlihat (greyed) saat sensor gagal baca** sementara, alih-alih hilang mendadak dari menu — menggunakan nilai last-known-good.
- **Timer terdaftar di `NSRunLoopCommonModes`** — polling tetap jalan walau menu sedang terbuka atau window sedang di-drag (bug umum menu-bar app yang berhenti update saat run loop masuk mode event-tracking).

## Arsitektur

```
src/
├── main.rs              # AppDelegate (NSApplicationDelegate + NSMenuDelegate),
│                         # status item, timer polling, menu build/update
├── values.rs             # FormatKind/FormatOptions, unit konversi & warna threshold
├── prefs/
│   └── mod.rs             # Window preferensi (AppKit)
├── settings/
│   ├── mod.rs              # Struct Settings + enum konfigurasi (serde)
│   └── defaults.rs         # Default value, load()/save() dari/ke TOML
└── sensors/
    ├── mod.rs               # SensorId/SensorGroup/SensorReading/SensorHistory
    ├── sys.rs                # Helper syscall dasar (sysctl dll.)
    ├── cpu.rs                # Sampler CPU (total + per-core), info statis (brand, cores)
    ├── memory.rs             # Memory & swap
    ├── disk.rs                # Storage usage
    ├── system.rs              # Uptime, load average, open files, threads/proses
    ├── network.rs             # Akumulasi throughput per-interface
    ├── wifi.rs                # CoreWLAN — link quality, RSSI, noise
    ├── smc.rs                 # Koneksi Apple SMC (System Management Controller)
    ├── thermal.rs             # Baca suhu/fan/voltase dari SMC via smc.rs
    ├── battery.rs             # Battery reading, health watcher, time-left estimator
    ├── gpu.rs                 # GPU utilization & info statis
    ├── display_refresh.rs     # Refresh rate display utama (Hz)
    └── public_ip.rs           # Watcher IP publik (polling interval, provider, flag)
```

Binary utama berjalan sebagai `NSApplicationActivationPolicy::Accessory` (tanpa ikon Dock, tanpa item menu bar App). Semua state UI (status item, menu, submenu kategori, timer) disimpan sebagai ivar `OnceCell`/`RefCell` di `AppDelegateIvars`, di-mutate langsung dari tick timer di main thread.

### Alur polling (tiap tick)

1. Baca ulang semua sensor (`system`, `cpu`, `memory`, `disk`, `network`, `wifi`, `battery`, `thermal`, `gpu`, `display_refresh`, `public_ip`).
2. Susun `SensorDescriptor` per metrik yang berhasil dibaca (dengan fallback last-known-good untuk sensor yang sempat gagal).
3. Bandingkan terhadap `SensorHistory` — kalau tidak ada perubahan dan bukan tick paksa (`force`), skip update UI (hemat kerja render).
4. Update judul status bar (`update_panel_text`) — mode ringkas kalau ada sensor yang di-pin.
5. Update/menu rebuild in-place (`rebuild_menu`) — baris pinned & baris detail kategori di-update via `setTitle`/`setState`, bukan dibongkar total, supaya submenu yang sedang terbuka tidak terganggu.

## Konfigurasi

File konfigurasi TOML tersimpan di:

```
~/Library/Application Support/pantau-app/config.toml
```

Field utama (lihat `src/settings/mod.rs` untuk skema lengkap):

| Field | Tipe | Keterangan |
|---|---|---|
| `hot_sensors` | `[String]` | Daftar key sensor yang di-pin ke status bar |
| `update_time` | `u32` | Interval polling (detik), default `1` |
| `position_in_panel` | enum | `FarLeft`/`Left`/`Center`/`Right`/`FarRight` |
| `use_higher_precision` | `bool` | Presisi angka lebih detail |
| `alphabetize` | `bool` | Urutkan baris pin secara alfabetis |
| `hide_zeros` | `bool` | Sembunyikan baris bernilai 0 (kecuali Fan) |
| `show_*` | `bool` | Toggle kategori (temperature/voltage/fan/memory/processor/system/storage/network/battery/gpu) |
| `unit` | enum | `Celsius` / `Fahrenheit` |
| `include_public_ip` | `bool` | Aktifkan pemantauan IP publik |
| `network_public_ip_interval` | `u32` | Interval polling IP publik (detik), default `300` |
| `network_public_ip_provider` | enum | `CoreCoding` / `MyIp` / `Ipify` |
| `network_public_ip_show_flag` | `bool` | Tampilkan emoji bendera negara |
| `storage_path` | `String` | Path volume yang dipantau, default `/` |
| `memory_measurement` / `storage_measurement` | enum | `Binary` (KiB/MiB) / `Decimal` (KB/MB) |
| `fixed_widths` | `bool` | Font monospaced-digit di judul status bar |
| `hide_icons` | `bool` | Sembunyikan ikon gauge status bar |
| `monitor_cmd` | `String` | Command dijalankan tombol "System Monitor" |
| `temperature_colors`, `fan_colors`, `memory_colors`, `processor_colors`, `system_colors`, `battery_colors`, `gpu_colors` | `[ColorBand]` | Threshold warna per kategori |

Kalau file konfigurasi tidak ada, tidak terbaca, atau korup, aplikasi otomatis fallback ke default (`Settings::default()`) tanpa crash.

## Build & Install

### Prasyarat

- macOS 11.0+ (`LSMinimumSystemVersion`)
- Rust toolchain (edition 2024)
- Xcode Command Line Tools (framework `CoreWLAN` di-link via `build.rs`)

### Build lokal

```bash
cargo build --release
```

### Install sebagai app bundle

```bash
./pkg/build.sh
```

Script ini akan:

1. `cargo build --release`
2. Rakit `Pantau.app` (dengan `Info.plist` dari `pkg/Info.plist`) di direktori temp
3. Ad-hoc codesign (`codesign --force --deep --sign -`)
4. Hapus versi lama di `/Applications/Pantau.app`, install versi baru
5. Buka aplikasinya

App bundle identifier: `dev.nuflakbrr.pantau-app`. Aplikasi berjalan sebagai `LSUIElement` (agent app — tidak muncul di Dock/Cmd+Tab).

## Dependencies

| Crate | Kegunaan |
|---|---|
| `objc2`, `objc2-app-kit`, `objc2-foundation` | Binding AppKit/Foundation native |
| `libc`, `mach2` | Syscall low-level (sysctl, mach host info) |
| `core-foundation-sys` | Interop Core Foundation (SMC, dll.) |
| `serde`, `toml` | Serialisasi konfigurasi |
| `directories` | Resolusi path config direktori standar OS |
| `ureq`, `serde_json` | HTTP client ringan untuk polling public IP |

## Testing

```bash
cargo test
```

## Known Limitations

- `menu_centered` adalah no-op di macOS (peninggalan skema GNOME) — AppKit selalu menempatkan `NSMenu` pada titik klik, tidak ada mode "centered" terpisah. Field tetap ada demi kompatibilitas struktur settings, ditandai "not applicable" di UI preferensi.
- Kategori menu dengan nol baris pada tick pertama tidak akan pernah mendapat header meskipun kemudian punya data — dalam praktiknya tidak masalah karena hardware asli selalu punya semua sensor tersedia sejak tick pertama.
- GPU utilization mengandalkan API publik yang tidak tersedia di semua model Mac — kalau tidak tersedia, baris menampilkan "GPU: Unavailable (no public API on this Mac)".
