fn main() {
    // Compile protobuf definitions for the Python sidecar gRPC interface
    tonic_build::configure()
        .build_server(false) // Rust is client only
        .compile_protos(&["../proto/sidecar.proto"], &["../proto"])
        .expect("Failed to compile protobuf definitions");

    // Generate Python protobuf stubs so the sidecar can import them.
    // This runs best-effort — a failure here is a warning, not a build error,
    // because the stubs may already exist or python3/grpcio-tools may not be
    // installed in the build environment.
    let proto_src = "../proto/sidecar.proto";
    let python_out = "../sidecar";
    let status = std::process::Command::new("python3")
        .args([
            "-m",
            "grpc_tools.protoc",
            &format!("-I{}", "../proto"),
            &format!("--python_out={python_out}"),
            &format!("--grpc_python_out={python_out}"),
            proto_src,
        ])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("cargo:warning=Generated Python protobuf stubs in {python_out}");
        }
        Ok(s) => {
            println!(
                "cargo:warning=Python protobuf generation exited with {s} — \
                 ensure grpcio-tools is installed (pip install grpcio-tools)"
            );
        }
        Err(e) => {
            println!(
                "cargo:warning=Could not run python3 for protobuf generation: {e} — \
                 Python stubs must be generated manually"
            );
        }
    }

    // Re-run this build script if the proto file changes.
    println!("cargo:rerun-if-changed={proto_src}");

    tauri_build::build();
}
