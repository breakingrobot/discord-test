//! Embeds the application icon and version info into the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        // Resource id 1 is what GPUI loads as the window / taskbar icon.
        res.set_icon_with_id("assets/app.ico", "1");
        res.set("FileDescription", "GPUI Discord");
        res.set("ProductName", "GPUI Discord");
        res.compile().expect("failed to embed Windows resources");
    }
}
