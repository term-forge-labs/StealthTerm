fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let ico_path = std::path::Path::new(&manifest_dir).join("../../assets/app.ico");

    // Ensure re-run when ico file changes
    println!("cargo:rerun-if-changed={}", ico_path.display());

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:warning=Building Windows resources with icon: {}", ico_path.display());

        let mut res = winresource::WindowsResource::new();
        res.set_icon(ico_path.to_str().unwrap());

        if let Err(e) = res.compile() {
            println!("cargo:warning=Failed to compile Windows resources: {}", e);
        } else {
            println!("cargo:warning=Windows resources compiled successfully");
        }
    }
}
