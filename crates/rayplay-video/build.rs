use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let header_path = manifest_dir.join("nvenc").join("wrapper.h");

    println!("cargo:rerun-if-changed={}", header_path.display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("nvenc").join("nvEncodeAPI.h").display()
    );

    let bindings = bindgen::Builder::default()
        .header(header_path.to_str().unwrap())
        .clang_arg(format!(
            "-I{}",
            manifest_dir.join("nvenc").to_str().unwrap()
        ))
        // Only generate NVENC types — not all of stdlib/windows.h
        .allowlist_type("_?NV_ENC.*")
        .allowlist_type("_?NV_ENCODE.*")
        .allowlist_type("_?NVENCSTATUS")
        .allowlist_type("_?GUID")
        .allowlist_type("_?NVENC_RECT")
        .allowlist_var("NVENCAPI.*")
        .allowlist_function("NvEncodeAPI.*")
        // Blocklist GUID statics — they're `static const` in C and bindgen
        // generates `extern "C" { static ... }` which won't link. We define
        // these as Rust const in nvenc_sys.rs instead.
        .blocklist_var("NV_ENC_CODEC_.*_GUID")
        .blocklist_var("NV_ENC_PRESET_.*_GUID")
        .blocklist_var("NV_ENC_H264_.*_GUID")
        .blocklist_var("NV_ENC_HEVC_.*_GUID")
        .blocklist_var("NV_ENC_AV1_.*_GUID")
        .blocklist_var("NV_ENC_CODEC_PROFILE_.*_GUID")
        // Derive useful traits
        .derive_debug(true)
        .derive_copy(true)
        .derive_default(true)
        .derive_eq(true)
        .derive_hash(false)
        .derive_partialeq(true)
        // Use core types
        .use_core()
        // Wrap unsafe ops in unsafe blocks (Rust 2024 edition requirement)
        .wrap_unsafe_ops(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .layout_tests(true)
        .generate()
        .expect("Failed to generate NVENC bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("nvenc_bindings.rs"))
        .expect("Failed to write NVENC bindings");
}
