//! Configure a real Metal surface repeatedly, without opening any user window/document.
#[cfg(all(target_os = "macos", feature = "wgpu"))]
#[allow(unsafe_code)] // The retained CAMetalLayer outlives the unsafe surface handle.
fn check() -> Result<(), String> {
    use eframe::wgpu;
    use objc2::rc::Retained;
    use objc2_core_graphics::{CGColorSpace, kCGColorSpaceSRGB};
    use objc2_quartz_core::CAMetalLayer;
    let layer = CAMetalLayer::new();
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::METAL, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let surface =
        unsafe { instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(Retained::as_ptr(&layer).cast_mut().cast())) }
            .map_err(|e| e.to_string())?;
    let adapter = wait(instance.request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() }))
        .map_err(|e| e.to_string())?;
    let (device, _queue) = wait(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| e.to_string())?;
    let mut config = surface.get_default_config(&adapter, 32, 32).ok_or("no surface configuration")?;
    let srgb = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).ok_or("sRGB is unavailable")?;
    for size in [32, 64, 128, 32] {
        config.width = size;
        config.height = size;
        // The real backend must restore the tag, including a resize-end configuration
        // with no subsequent application repaint.
        layer.setColorspace(None);
        surface.configure(&device, &config);
        if layer.colorspace().as_deref() != Some(&srgb) {
            return Err(format!("Metal surface {size} lost colour matching"));
        }
    }
    drop(surface);
    Ok(())
}

#[cfg(all(target_os = "macos", feature = "wgpu"))]
fn wait<T>(future: impl std::future::Future<Output = T>) -> T {
    struct Wake(std::thread::Thread);
    impl std::task::Wake for Wake {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(std::sync::Arc::new(Wake(std::thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => return value,
            std::task::Poll::Pending => std::thread::park(),
        }
    }
}

fn main() -> std::process::ExitCode {
    #[cfg(all(target_os = "macos", feature = "wgpu"))]
    {
        if let Err(error) = check() {
            eprintln!("Native display colour check failed: {error}");
            return std::process::ExitCode::FAILURE;
        }
        println!("Native Metal display colour: all four surface configurations passed without repaint");
    }
    #[cfg(not(all(target_os = "macos", feature = "wgpu")))]
    println!("Skipped: the native display probe requires macOS with wgpu");
    std::process::ExitCode::SUCCESS
}
