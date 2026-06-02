fn main() {
    println!("cargo:rerun-if-env-changed=OPENLOGI_UPDATE_MANIFEST_URL");

    // Embed the app icon as resource id 1. GPUI's Windows backend loads the
    // window/taskbar icon from `LoadImageW(module, PCWSTR(1))`, and the tray
    // module loads the same id for the notification-area icon.
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=winres/openlogi.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon_with_id("winres/openlogi.ico", "1");
        if let Err(e) = res.compile() {
            println!("cargo:warning=failed to embed Windows icon: {e}");
        }
    }
}
