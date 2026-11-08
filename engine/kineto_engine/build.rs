fn main() {
    let manifest_dir = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR should be set"),
    );

    // Step 7.2: generate Rust FlatBuffers bindings from the runtime schema.
    flatbuffers_build::BuilderOptions::new_with_files(["../../schema/kineto_runtime.fbs"])
        .compile()
        .expect("flatc --rust failed for kineto_runtime.fbs");

    println!("cargo:rerun-if-changed=../../schema/kineto_runtime.fbs");
    println!("cargo:rerun-if-changed=src/ffi/c.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");

    // Step 7.10: generate the public C ABI header consumed by Swift/Dart.
    let config = cbindgen::Config::from_file(manifest_dir.join("cbindgen.toml"))
        .expect("cbindgen.toml should load");
    cbindgen::Builder::new()
        .with_crate(&manifest_dir)
        .with_config(config)
        .generate()
        .expect("cbindgen failed for kineto_engine")
        .write_to_file(manifest_dir.join("kineto_engine.h"));
}
