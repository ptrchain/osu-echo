#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set("FileDescription", "osu-echo - Local osu! Server");
    res.set("ProductName", "osu-echo");
    res.set("OriginalFilename", "osu-echo.exe");
    res.set("LegalCopyright", "Copyright (c) 2026 Rizky");
    if let Err(e) = res.compile() {
        eprintln!("Failed to compile Windows resources: {}", e);
    }
}

#[cfg(not(windows))]
fn main() {}
