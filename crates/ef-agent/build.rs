fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protos = ["../../proto/agent.proto"];

    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .build_server(false)
        .build_client(true)
        .compile_protos(&protos, &["../../proto"])?;

    for proto in &protos {
        println!("cargo:rerun-if-changed={proto}");
    }

    Ok(())
}
