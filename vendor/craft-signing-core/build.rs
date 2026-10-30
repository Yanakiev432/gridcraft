#[cfg(target_os = "windows")]
fn main() {
    cc::Build::new()
        .cpp(true)
        .file("native/opc_native.cpp")
        .flag_if_supported("/std:c++17")
        .warnings(true)
        .compile("craft_opc_native");
    println!("cargo:rustc-link-lib=ole32");
    println!("cargo:rustc-link-lib=crypt32");
    println!("cargo:rustc-link-lib=ncrypt");
    println!("cargo:rustc-link-lib=urlmon");
    println!("cargo:rustc-link-lib=user32");
    println!("cargo:rerun-if-changed=native/opc_native.cpp");
}
#[cfg(not(target_os = "windows"))]
fn main() {}
