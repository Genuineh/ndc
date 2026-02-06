//! Build script for proto compilation
//!
//! Compiles .proto files when grpc feature is enabled
//! Uses tonic_build to generate both messages and gRPC service clients

fn main() {
    // 检查是否启用了 grpc feature
    if cfg!(feature = "grpc") {
        // 使用 CARGO_MANIFEST_DIR 获取 crate 根目录
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let proto_path = std::path::Path::new(&manifest_dir).join("protos/ndc.proto");

        println!("cargo:rerun-if-changed={}", proto_path.display());
        println!("cargo:rerun-if-env-changed=CARGO_MANIFEST_DIR");

        if proto_path.exists() {
            println!("Proto file found: {:?}", proto_path);
            let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
            let includes = std::path::Path::new(&manifest_dir).join("protos");
            let out_dir = std::env::var("OUT_DIR").unwrap();
            let out_dir = std::path::Path::new(&out_dir);

            println!("OUT_DIR: {:?}", out_dir);

            tonic_build::configure()
                .out_dir(out_dir.to_str().unwrap())
                .compile_protos(
                    &[proto_path.to_str().unwrap()],
                    &[includes.to_str().unwrap()],
                )
                .expect("Failed to compile proto files");

            println!("Proto compilation successful");
        } else {
            println!("Warning: proto file not found at {}", proto_path.display());
        }
    }
}
