fn main() {
    // Compile protobuf definitions for the Python sidecar gRPC interface
    tonic_build::configure()
        .build_server(false) // Rust is client only
        .compile_protos(&["../proto/sidecar.proto"], &["../proto"])
        .expect("Failed to compile protobuf definitions");

    tauri_build::build();
}
