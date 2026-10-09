//! Which graphics adapter the window renders with (#306, #502), and what happens when it can't
//! show the window.
//!
//! The canvas is rasterized on the CPU; the GPU only composites it and draws the UI, so any
//! adapter that can present to the window will do. [`adapter_order`] ranks the adapters: those that
//! can't present to the window's surface never, then the one `WGPU_ADAPTER_NAME` names, hardware
//! before software, the native backends (Vulkan, Metal, DX12) before OpenGL, the power preference
//! ([`power_preference`]) among the rest, and a GPU's DX12 adapter before its Vulkan one (#545).
//! eframe is handed [`selector`], which takes the first of that order.
//!
//! An adapter can report that it presents to the window and still fail once it does: on a hybrid
//! Linux desktop under Wayland, the compositor runs on one GPU and may refuse the frame buffers
//! another GPU allocates, which kills the window's Wayland connection; egui-wgpu then panics while
//! configuring the surface (#502). Nothing in the process can show a window after that, so when
//! the graphics fail while the window is starting up, [`finish`] starts the app again without
//! that adapter, as the next one in the order would be tried. Each restart leaves out one more
//! adapter, so it ends when none is left.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use eframe::egui_wgpu::NativeAdapterSelectorMethod;
use eframe::wgpu::{self, Backend, DeviceType, PowerPreference};

/// The adapters a restart leaves out, those the window failed on, as comma-separated [`key`]s
/// (`Vulkan:1002:164e`, `Metal:Intel Iris Pro Graphics`). Set by the app itself when it starts
/// again.
pub const SKIP_ENV: &str = "VECTORCRAFT_GPU_SKIP";

/// wgpu's variable for choosing an adapter by (part of) its name, any case.
const NAME_ENV: &str = "WGPU_ADAPTER_NAME";

/// Where to read how to choose another adapter, for the error that ends the app.
const HELP: &str =
    "to choose a graphics adapter, see https://github.com/storytold/vectorcraft/blob/main/docs/development.md#desktop-graphics-processor";

/// Frames the UI ran before the failure, below which it was still starting up. The failure in
/// #502 came on the first or second frame; later, the adapter has proved itself, and a lost
/// window is something else (such as the compositor restarting).
pub const STARTUP_FRAMES: u64 = 10;

/// The preference used when Preferences say Automatic. Windows and macOS show frames from any GPU
/// (the integrated one avoids the flicker of #306). Elsewhere, the system's own order: Mesa's
/// device-select layer puts the GPU the desktop runs on first (the integrated one on hybrid
/// laptops), and a Wayland compositor may not show frames from another GPU (#502).
pub const AUTOMATIC: PowerPreference = if cfg!(any(windows, target_os = "macos")) { PowerPreference::LowPower } else { PowerPreference::None };

/// The power preference that orders the adapters: the `WGPU_POWER_PREF` environment variable
/// (`low`, `high`, `none`) when set, otherwise Preferences › Performance › Graphics Processor
/// (`gpuPreference`). Automatic, and any value this version doesn't know (0.5.0's default
/// `powerSaving`, a newer or damaged preference file), is [`AUTOMATIC`].
pub fn power_preference(pref: Option<&str>, env: Option<PowerPreference>) -> PowerPreference {
    match (env, pref) {
        (Some(p), _) => p,
        (None, Some("highPerformance")) => PowerPreference::HighPerformance,
        (None, Some("lowPower")) => PowerPreference::LowPower,
        (None, _) => AUTOMATIC,
    }
}

/// One adapter, as [`adapter_order`] sees it.
#[derive(Clone, Debug)]
pub struct Candidate {
    /// Identifies the adapter across a restart ([`key`]).
    pub key: String,
    pub name: String,
    pub backend: Backend,
    pub device_type: DeviceType,
    /// Reports that it can present to the window's surface.
    pub presents: bool,
}

impl Candidate {
    fn new(adapter: &wgpu::Adapter, surface: Option<&wgpu::Surface<'_>>) -> Self {
        let info = adapter.get_info();
        Self {
            key: key(info.backend, info.vendor, info.device, &info.name),
            presents: surface.is_none_or(|s| adapter.is_surface_supported(s)),
            name: info.name,
            backend: info.backend,
            device_type: info.device_type,
        }
    }
}

/// `backend:vendor:device`, e.g. `Vulkan:1002:164e` (PCI ids, as `MESA_VK_DEVICE_SELECT` takes them).
/// Metal reports no ids (`0000:0000` for every GPU), so an adapter without them is `backend:name`
/// (`Metal:Intel Iris Pro Graphics`): otherwise leaving out the one that failed would leave out
/// every GPU of a dual-GPU Mac (#651). Commas, which separate [`SKIP_ENV`]'s keys, become spaces.
fn key(backend: Backend, vendor: u32, device: u32, name: &str) -> String {
    if vendor == 0 && device == 0 {
        format!("{backend:?}:{}", name.replace(',', " ").trim())
    } else {
        format!("{backend:?}:{vendor:04x}:{device:04x}")
    }
}

/// The order to try `candidates` in (their indices): only those that present to the window and
/// aren't in `skip`; the one whose name contains `named` (any case) first; hardware before
/// software; native backends before OpenGL; then by `power`; then DX12 before Vulkan, keeping the
/// system's order among equals (all of it for [`PowerPreference::None`]). Windows lists each GPU
/// under both, and Intel's Vulkan driver made the whole window flicker black where DX12 and OpenGL
/// didn't (#545); elsewhere there is no DX12 adapter, so the order is unchanged.
pub fn adapter_order(candidates: &[Candidate], power: PowerPreference, named: Option<&str>, skip: &[String]) -> Vec<usize> {
    let named = named.map(str::to_lowercase).filter(|n| !n.is_empty());
    let mut order: Vec<(usize, &Candidate)> = candidates.iter().enumerate().filter(|(_, c)| c.presents && !skip.contains(&c.key)).collect();
    // Stable, so equals keep the system's order.
    order.sort_by_key(|(_, c)| {
        let unnamed = named.as_ref().is_some_and(|n| !c.name.to_lowercase().contains(n));
        (unnamed, c.device_type == DeviceType::Cpu, c.backend == Backend::Gl, power_rank(c.device_type, power), c.backend == Backend::Vulkan)
    });
    order.into_iter().map(|(i, _)| i).collect()
}

/// wgpu's own ranking for a power preference: the preferred kind of GPU, the other kind, unknown,
/// virtual, software.
fn power_rank(t: DeviceType, power: PowerPreference) -> u8 {
    match (power, t) {
        (PowerPreference::None, _) => 0,
        (PowerPreference::LowPower, DeviceType::IntegratedGpu) | (PowerPreference::HighPerformance, DeviceType::DiscreteGpu) => 0,
        (_, DeviceType::IntegratedGpu | DeviceType::DiscreteGpu) => 1,
        (_, DeviceType::Other) => 2,
        (_, DeviceType::VirtualGpu) => 3,
        (_, DeviceType::Cpu) => 4,
    }
}

/// The adapters [`SKIP_ENV`] lists.
pub fn skipped() -> Vec<String> {
    std::env::var(SKIP_ENV).map(|v| parse_skip(&v)).unwrap_or_default()
}

fn parse_skip(v: &str) -> Vec<String> {
    v.split(',').map(str::trim).filter(|k| !k.is_empty()).map(String::from).collect()
}

/// What the window's start-up leaves for [`finish`].
#[derive(Default)]
pub struct Startup {
    /// The key of the adapter [`selector`] picked.
    adapter: OnceLock<String>,
    /// The UI's context, which counts the frames run.
    ui: OnceLock<egui::Context>,
}

impl Startup {
    /// The app was created on `ctx`.
    pub fn created(&self, ctx: &egui::Context) {
        // Set once; the window is created once.
        let _ = self.ui.set(ctx.clone());
    }
}

/// eframe's adapter choice: the first of [`adapter_order`] for `power`, `WGPU_ADAPTER_NAME` and the
/// adapters a restart left out. Every adapter and the choice are logged, for bug reports.
pub fn selector(power: PowerPreference, startup: Arc<Startup>) -> NativeAdapterSelectorMethod {
    let named = std::env::var(NAME_ENV).ok();
    let skip = skipped();
    Arc::new(move |adapters, surface| {
        let candidates: Vec<Candidate> = adapters.iter().map(|a| Candidate::new(a, surface)).collect();
        let order = adapter_order(&candidates, power, named.as_deref(), &skip);
        let listed: Vec<String> = candidates
            .iter()
            .map(|c| {
                let note = if !c.presents {
                    ", can't show the window"
                } else if skip.contains(&c.key) {
                    ", failed before"
                } else {
                    ""
                };
                format!("{} ({:?}, {:?}, {}{note})", c.name.trim(), c.backend, c.device_type, c.key)
            })
            .collect();
        log::info!("graphics adapters: {}", listed.join("; "));
        let first = order.first().and_then(|&i| adapters.get(i).zip(candidates.get(i)));
        let Some((adapter, chosen)) = first else {
            return Err(format!("no graphics adapter can show the window (power preference {power:?}, {} adapters)", adapters.len()));
        };
        // Set once; eframe picks the adapter once.
        let _ = startup.adapter.set(chosen.key.clone());
        Ok(adapter.clone())
    })
}

/// The last panic came from the graphics stack (see [`watch_panics`]).
static GRAPHICS_PANIC: AtomicBool = AtomicBool::new(false);

/// Note whether each panic comes from the graphics stack, so [`finish`] can tell a failing adapter
/// from a bug. The default hook still prints it.
pub fn watch_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        GRAPHICS_PANIC.store(info.location().is_some_and(|l| in_graphics_stack(l.file())), Ordering::Relaxed);
        default(info);
    }));
}

/// A source file of wgpu (`wgpu-core-30.0.1/src/…`, `wgpu-hal-…`), egui-wgpu or naga.
fn in_graphics_stack(file: &str) -> bool {
    file.split(['/', '\\']).any(|dir| ["wgpu-", "egui-wgpu-", "naga-"].iter().any(|p| dir.starts_with(p)))
}

/// The adapter to start again without: the one picked, when the graphics failed while the window
/// was starting up and it wasn't already left out.
fn restart_without<'a>(graphics_failed: bool, frames: u64, picked: Option<&'a str>, skip: &[String]) -> Option<&'a str> {
    picked.filter(|p| graphics_failed && frames < STARTUP_FRAMES && !skip.iter().any(|s| s == p))
}

/// After the window's run: `Ok` when it closed normally or the app started again (see the module
/// docs), otherwise why it failed. `outcome` is eframe's result, or the message of a panic that
/// ended it.
pub fn finish(outcome: Result<eframe::Result, String>, startup: &Startup) -> Result<(), String> {
    let (why, graphics_failed) = match outcome {
        Ok(Ok(())) => return Ok(()),
        Ok(Err(e @ eframe::Error::Wgpu(_))) => (e.to_string(), true),
        Ok(Err(e)) => (e.to_string(), false),
        Err(panic) => (panic, GRAPHICS_PANIC.load(Ordering::Relaxed)),
    };
    let frames = startup.ui.get().map_or(0, egui::Context::cumulative_frame_nr);
    let mut skip = skipped();
    let Some(failed) = restart_without(graphics_failed, frames, startup.adapter.get().map(String::as_str), &skip) else {
        return Err(if graphics_failed { format!("{why} ({HELP})") } else { why });
    };
    log::error!("the window failed on graphics adapter {failed} ({why}): starting again without it");
    skip.push(failed.to_string());
    restart(&skip.join(",")).map_err(|e| format!("{why}; starting again failed: {e}"))
}

/// Start the app again with the same arguments, leaving out the adapters in `skip`. On Unix the
/// new app replaces this process (same process id, so an AppImage keeps its files mounted), and
/// this returns only on failure; on Windows it starts beside this one, which then exits.
fn restart(skip: &str) -> std::io::Result<()> {
    log::logger().flush();
    let mut c = std::process::Command::new(std::env::current_exe()?);
    c.args(std::env::args_os().skip(1)).env(SKIP_ENV, skip);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        Err(c.exec())
    }
    #[cfg(not(unix))]
    c.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(name: &str, backend: Backend, device_type: DeviceType) -> Candidate {
        Candidate { key: format!("{backend:?}:{name}"), name: name.into(), backend, device_type, presents: true }
    }

    fn names(c: &[Candidate], order: &[usize]) -> Vec<String> {
        order.iter().map(|&i| c[i].name.clone()).collect()
    }

    /// The machine of #502 as Vulkan lists it with Mesa's device-select layer: the GPU KDE's
    /// compositor runs on (NVIDIA) first, then the Ryzen's integrated GPU, Mesa's software
    /// renderer, and OpenGL.
    fn issue_502() -> Vec<Candidate> {
        vec![
            gpu("NVIDIA GeForce RTX 5070", Backend::Vulkan, DeviceType::DiscreteGpu),
            gpu("AMD Ryzen 9 7900X (RADV RAPHAEL_MENDOCINO)", Backend::Vulkan, DeviceType::IntegratedGpu),
            gpu("llvmpipe (LLVM 20.1.8, 256 bits)", Backend::Vulkan, DeviceType::Cpu),
            gpu("NVIDIA GeForce RTX 5070/PCIe/SSE2", Backend::Gl, DeviceType::Other),
        ]
    }

    #[test]
    fn automatic_keeps_the_systems_order_on_linux_and_saves_power_elsewhere() {
        let c = issue_502();
        let order = adapter_order(&c, power_preference(None, None), None, &[]);
        let first = &c[order[0]].name;
        if cfg!(any(windows, target_os = "macos")) {
            assert_eq!(first, "AMD Ryzen 9 7900X (RADV RAPHAEL_MENDOCINO)");
        } else {
            // #502: the integrated GPU couldn't show frames to a compositor on the NVIDIA GPU.
            assert_eq!(first, "NVIDIA GeForce RTX 5070");
        }
    }

    #[test]
    fn each_preference_orders_the_adapters_then_falls_back_to_opengl_and_software() {
        let c = issue_502();
        let order = |power| names(&c, &adapter_order(&c, power, None, &[]));
        assert_eq!(
            order(PowerPreference::None),
            [
                "NVIDIA GeForce RTX 5070",
                "AMD Ryzen 9 7900X (RADV RAPHAEL_MENDOCINO)",
                "NVIDIA GeForce RTX 5070/PCIe/SSE2",
                "llvmpipe (LLVM 20.1.8, 256 bits)"
            ]
        );
        assert_eq!(
            order(PowerPreference::LowPower),
            [
                "AMD Ryzen 9 7900X (RADV RAPHAEL_MENDOCINO)",
                "NVIDIA GeForce RTX 5070",
                "NVIDIA GeForce RTX 5070/PCIe/SSE2",
                "llvmpipe (LLVM 20.1.8, 256 bits)"
            ]
        );
        assert_eq!(
            order(PowerPreference::HighPerformance),
            [
                "NVIDIA GeForce RTX 5070",
                "AMD Ryzen 9 7900X (RADV RAPHAEL_MENDOCINO)",
                "NVIDIA GeForce RTX 5070/PCIe/SSE2",
                "llvmpipe (LLVM 20.1.8, 256 bits)"
            ]
        );
    }

    /// A restart leaves out the adapter the window failed on, so Power Saving on the machine of
    /// #502 ends up on the NVIDIA GPU, and once every adapter failed there is nothing to try.
    /// A dual-GPU Mac reports both GPUs under Metal with no PCI ids (#651): their keys still
    /// differ, so when the integrated GPU fails, the restart renders on the discrete one.
    #[test]
    fn gpus_without_pci_ids_are_told_apart_by_name() {
        let mac = |name: &str, device_type| Candidate {
            key: key(Backend::Metal, 0, 0, name),
            name: name.into(),
            backend: Backend::Metal,
            device_type,
            presents: true,
        };
        let c = vec![mac("NVIDIA GeForce GT 750M", DeviceType::DiscreteGpu), mac("Intel Iris Pro Graphics", DeviceType::IntegratedGpu)];
        assert_eq!((c[0].key.as_str(), c[1].key.as_str()), ("Metal:NVIDIA GeForce GT 750M", "Metal:Intel Iris Pro Graphics"));
        let first = adapter_order(&c, PowerPreference::LowPower, None, &[]);
        assert_eq!(names(&c, &first)[0], "Intel Iris Pro Graphics");
        let skip = parse_skip(&c[first[0]].key);
        assert_eq!(names(&c, &adapter_order(&c, PowerPreference::LowPower, None, &skip)), ["NVIDIA GeForce GT 750M"]);
        // A comma in a name can't split the list.
        assert_eq!(parse_skip(&key(Backend::Gl, 0, 0, "Mesa, llvmpipe")), ["Gl:Mesa  llvmpipe"]);
    }

    #[test]
    fn a_restart_tries_the_next_adapter() {
        let c = issue_502();
        let mut skip = vec![];
        let mut tried = vec![];
        while let Some(&i) = adapter_order(&c, PowerPreference::LowPower, None, &skip).first() {
            tried.push(c[i].name.clone());
            skip.push(c[i].key.clone());
        }
        assert_eq!(
            tried,
            [
                "AMD Ryzen 9 7900X (RADV RAPHAEL_MENDOCINO)",
                "NVIDIA GeForce RTX 5070",
                "NVIDIA GeForce RTX 5070/PCIe/SSE2",
                "llvmpipe (LLVM 20.1.8, 256 bits)"
            ]
        );
    }

    #[test]
    fn adapters_that_cannot_present_to_the_window_are_never_tried() {
        let mut c = issue_502();
        c[0].presents = false;
        c[3].presents = false;
        for power in [PowerPreference::None, PowerPreference::LowPower, PowerPreference::HighPerformance] {
            let order = adapter_order(&c, power, None, &[]);
            assert!(!order.contains(&0) && !order.contains(&3), "{power:?}: {order:?}");
            assert_eq!(order.len(), 2);
        }
        for p in &mut c {
            p.presents = false;
        }
        assert!(adapter_order(&c, PowerPreference::None, None, &[]).is_empty());
        assert!(adapter_order(&[], PowerPreference::HighPerformance, None, &[]).is_empty());
    }

    #[test]
    fn wgpu_adapter_name_picks_an_adapter_first_and_the_rest_stay_as_fallbacks() {
        let c = issue_502();
        let order = adapter_order(&c, PowerPreference::HighPerformance, Some("radv"), &[]);
        assert_eq!(c[order[0]].name, "AMD Ryzen 9 7900X (RADV RAPHAEL_MENDOCINO)");
        assert_eq!(order.len(), 4);
        // Any case; every match comes before the rest, natively first.
        let order = adapter_order(&c, PowerPreference::LowPower, Some("GeForce"), &[]);
        assert_eq!(names(&c, &order[..2]), ["NVIDIA GeForce RTX 5070", "NVIDIA GeForce RTX 5070/PCIe/SSE2"]);
        // An empty name or one nothing matches changes nothing.
        for named in ["", "no such gpu"] {
            assert_eq!(adapter_order(&c, PowerPreference::LowPower, Some(named), &[]), adapter_order(&c, PowerPreference::LowPower, None, &[]));
        }
        // A named adapter that failed is left out like any other.
        let skip = vec![c[1].key.clone()];
        assert_eq!(adapter_order(&c, PowerPreference::None, Some("radv"), &skip)[0], 0);
    }

    /// A Windows machine lists every GPU twice (Vulkan and DX12): power saving still takes the
    /// integrated GPU first, as wgpu's own choice did (#306), each GPU through DX12 first (#545).
    #[test]
    fn hybrid_laptop_on_windows_renders_on_the_integrated_gpu_with_power_saving() {
        let c = vec![
            gpu("Intel(R) Arc(TM) A370M", Backend::Vulkan, DeviceType::DiscreteGpu),
            gpu("Intel(R) Iris(R) Xe Graphics", Backend::Vulkan, DeviceType::IntegratedGpu),
            gpu("Intel(R) Arc(TM) A370M", Backend::Dx12, DeviceType::DiscreteGpu),
            gpu("Intel(R) Iris(R) Xe Graphics", Backend::Dx12, DeviceType::IntegratedGpu),
            gpu("Microsoft Basic Render Driver", Backend::Dx12, DeviceType::Cpu),
        ];
        let order = adapter_order(&c, PowerPreference::LowPower, None, &[]);
        assert_eq!(order, [3, 1, 2, 0, 4]);
        assert_eq!(adapter_order(&c, PowerPreference::HighPerformance, None, &[]), [2, 0, 3, 1, 4]);
    }

    /// The machine of #545: one Intel GPU, whose Vulkan driver made the window flicker black.
    /// DX12 comes first whatever the preference; Vulkan stays a fallback, and `WGPU_ADAPTER_NAME`
    /// or a restart leaving DX12 out still reach it.
    #[test]
    fn a_windows_gpu_renders_through_dx12_before_vulkan() {
        let c = vec![
            gpu("Intel(R) Graphics", Backend::Vulkan, DeviceType::IntegratedGpu),
            gpu("Intel(R) Graphics", Backend::Dx12, DeviceType::IntegratedGpu),
            gpu("Microsoft Basic Render Driver", Backend::Dx12, DeviceType::Cpu),
            gpu("Intel(R) Graphics", Backend::Gl, DeviceType::Other),
        ];
        for power in [PowerPreference::LowPower, PowerPreference::HighPerformance, PowerPreference::None] {
            assert_eq!(adapter_order(&c, power, None, &[]), [1, 0, 3, 2], "{power:?}");
        }
        assert_eq!(adapter_order(&c, PowerPreference::LowPower, None, &[c[1].key.clone()]), [0, 3, 2], "DX12 failed: Vulkan next");
    }

    #[test]
    fn power_preference_follows_the_environment_then_the_preference() {
        assert_eq!(power_preference(None, None), AUTOMATIC);
        assert_eq!(power_preference(Some("automatic"), None), AUTOMATIC);
        assert_eq!(power_preference(Some("lowPower"), None), PowerPreference::LowPower);
        assert_eq!(power_preference(Some("highPerformance"), None), PowerPreference::HighPerformance);
        // 0.5.0's default, and values this version doesn't know (a newer or damaged file).
        for old in ["powerSaving", "turbo", ""] {
            assert_eq!(power_preference(Some(old), None), AUTOMATIC, "{old}");
        }
        // WGPU_POWER_PREF wins over the preference, either way.
        assert_eq!(power_preference(Some("lowPower"), Some(PowerPreference::HighPerformance)), PowerPreference::HighPerformance);
        assert_eq!(power_preference(Some("highPerformance"), Some(PowerPreference::LowPower)), PowerPreference::LowPower);
        assert_eq!(power_preference(Some("lowPower"), Some(PowerPreference::None)), PowerPreference::None);
        assert_eq!(AUTOMATIC, if cfg!(any(windows, target_os = "macos")) { PowerPreference::LowPower } else { PowerPreference::None });
    }

    /// The preference the engine saves is the one the app reads back before the window opens.
    #[test]
    fn the_saved_engine_preference_is_found() {
        let mut prefs = vectorcraft_engine::Prefs::default();
        let saved = prefs.to_json();
        assert_eq!(saved.get("gpuPreference").and_then(serde_json::Value::as_str), Some("automatic"));
        for (value, power) in [("lowPower", PowerPreference::LowPower), ("highPerformance", PowerPreference::HighPerformance)] {
            prefs.gpu_preference = value.into();
            let saved = prefs.to_json();
            assert_eq!(power_preference(saved.get("gpuPreference").and_then(serde_json::Value::as_str), None), power);
        }
        // Every choice Preferences offer means something here.
        for (value, _) in vectorcraft_engine::cmd::prefscmds::GPU_PREFERENCES {
            assert!(["automatic", "lowPower", "highPerformance"].contains(value), "{value}");
        }
    }

    #[test]
    fn skipped_adapters_parse_from_a_comma_separated_list() {
        assert_eq!(parse_skip("Vulkan:1002:164e, Gl:10de:2f04,,"), ["Vulkan:1002:164e", "Gl:10de:2f04"]);
        assert!(parse_skip("").is_empty());
        assert_eq!(key(Backend::Vulkan, 0x1002, 0x164e, "AMD Radeon"), "Vulkan:1002:164e");
        assert_eq!(key(Backend::Gl, 0x10de, 0x2f04, "NVIDIA"), "Gl:10de:2f04");
    }

    #[test]
    fn panics_in_wgpu_egui_wgpu_and_naga_are_graphics_failures() {
        for file in [
            "/home/u/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/egui-wgpu-0.36.2/src/winit.rs",
            r"C:\Users\u\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\wgpu-core-30.0.1\src\device\mod.rs",
            "/rustc/deps/wgpu-hal-30.0.1/src/vulkan/adapter.rs",
            "naga-30.0.0/src/back/spv/writer.rs",
        ] {
            assert!(in_graphics_stack(file), "{file}");
        }
        for file in [
            "apps/vectorcraft/src/main.rs",
            "/home/wgpu/vectorcraft/crates/ui-egui/src/lib.rs",
            "egui-0.36.2/src/context.rs",
            "winit-0.30.12/src/x.rs",
        ] {
            assert!(!in_graphics_stack(file), "{file}");
        }
    }

    #[test]
    fn only_a_graphics_failure_while_starting_up_restarts_without_the_adapter() {
        let amd = Some("Vulkan:1002:164e");
        assert_eq!(restart_without(true, 0, amd, &[]), amd);
        assert_eq!(restart_without(true, STARTUP_FRAMES - 1, amd, &[]), amd);
        // After start-up the adapter has shown frames; a lost window is something else.
        assert_eq!(restart_without(true, STARTUP_FRAMES, amd, &[]), None);
        // A bug outside the graphics, or no adapter picked: nothing another adapter would change.
        assert_eq!(restart_without(false, 0, amd, &[]), None);
        assert_eq!(restart_without(true, 0, None, &[]), None);
        // Never the same adapter twice, so restarts end.
        assert_eq!(restart_without(true, 0, amd, &["Vulkan:1002:164e".into()]), None);
    }

    #[test]
    fn a_normal_exit_or_another_failure_does_not_restart() {
        let startup = Startup::default();
        assert_eq!(finish(Ok(Ok(())), &startup), Ok(()));
        // No adapter was picked (none can show the window): the error is returned, nothing restarts.
        let none =
            eframe::Error::Wgpu(eframe::egui_wgpu::WgpuError::CustomNativeAdapterSelectionError("no graphics adapter can show the window".into()));
        assert_eq!(
            finish(Ok(Err(none)), &startup),
            Err(format!("WGPU error: Adapter selection failed: no graphics adapter can show the window ({HELP})"))
        );
        let _ = startup.adapter.set("Vulkan:1002:164e".into());
        GRAPHICS_PANIC.store(false, Ordering::Relaxed);
        assert_eq!(finish(Err("a bug".into()), &startup), Err("a bug".into()));
    }
}
