//! Compiles the vendored Rubber Band Library.
//!
//! Upstream ships `single/RubberBandSingle.cpp`, one translation unit that
//! `#include`s the rest of the sources and needs no external library: the
//! built-in FFT and resampler everywhere, Apple's vDSP on macOS, which is why
//! the Accelerate framework is linked there. Vendored rather than found on the
//! system so that a Windows runner, a macOS runner and this machine all build
//! the same code.

fn main() {
    println!("cargo:rerun-if-changed=vendor/rubberband");
    #[cfg(feature = "rubberband")]
    build();
}

/// Only compiled with the feature, because `cc` is only a dependency with it:
/// a build without Rubber Band must not need a C++ compiler at all.
#[cfg(feature = "rubberband")]
fn build() {
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .include("vendor/rubberband")
        .file("vendor/rubberband/single/RubberBandSingle.cpp")
        // Upstream's own warnings are upstream's business, and there are a
        // lot of them; ours are still on for our code.
        .warnings(false);

    build.compile("rubberband");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-lib=framework=Accelerate");
    }
}
