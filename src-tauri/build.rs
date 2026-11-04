fn main() {
    // screencapturekit's Swift objects reference @rpath/libswift_Concurrency.dylib.
    // Those rpaths stay on the dependency and never reach this binary. On macOS the
    // concurrency runtime lives in /usr/lib/swift, including the dyld shared cache.
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    tauri_build::build()
}
