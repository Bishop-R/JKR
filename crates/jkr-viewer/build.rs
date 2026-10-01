//! Main-thread stack for Windows builds. Windows reserves 1 MiB for a program's main
//! thread where Linux reserves 8 MiB; the client overflows 1 MiB while loading a map,
//! so Windows binaries are linked with the Linux size.

const MAIN_STACK_BYTES: u32 = 8 * 1024 * 1024;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
        Ok("msvc") => println!("cargo:rustc-link-arg-bins=/STACK:{MAIN_STACK_BYTES}"),
        Ok("gnu") => println!("cargo:rustc-link-arg-bins=-Wl,--stack,{MAIN_STACK_BYTES}"),
        _ => {}
    }
}
