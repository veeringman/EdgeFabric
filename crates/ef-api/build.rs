fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protos = [
        "../../proto/device.proto",
        "../../proto/session.proto",
        "../../proto/test.proto",
        "../../proto/agent.proto",
    ];

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    let descriptor_path = out_dir.join("edgefabric_descriptor.bin");

    // Older protoc versions need the experimental flag for proto3 optional fields.
    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(&descriptor_path)
        .compile_protos(&protos, &["../../proto"])
        .map_err(|e| {
            eprintln!("tonic-build failed: {e}");
            e
        })?;

    // Rerun if any proto changes
    for proto in &protos {
        println!("cargo:rerun-if-changed={proto}");
    }

    Ok(())
}
