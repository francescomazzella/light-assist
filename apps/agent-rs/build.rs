//! Gives the Windows exe its icon and version information, which Explorer, the Start menu and
//! Settings → Apps show.

fn main() {
    println!("cargo:rerun-if-changed=assets/light-assist.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/light-assist.ico")
            .set("ProductName", "Light Assist")
            .set("FileDescription", "Light Assist")
            .set("CompanyName", "Francesco Mazzella")
            .set("LegalCopyright", "Francesco Mazzella");
        if let Err(error) = resource.compile() {
            println!("cargo:warning=the exe has no icon: {error}");
        }
    }
}
