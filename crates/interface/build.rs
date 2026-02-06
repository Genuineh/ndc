//! Build script for proto compilation
//!
//! Compiles .proto files when grpc feature is enabled

fn main() {
    // 检查是否启用了 grpc feature
    if cfg!(feature = "grpc") {
        // 使用 CARGO_MANIFEST_DIR 获取 crate 根目录
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let proto_path = std::path::Path::new(&manifest_dir).join("protos/ndc.proto");

        println!("cargo:rerun-if-changed={}", proto_path.display());

        if proto_path.exists() {
            // 使用 prost_build
            let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
            let includes = std::path::Path::new(&manifest_dir).join("protos");

            prost_build::Config::new()
                .out_dir("src/generated")
                .compile_protos(&[proto_path.to_str().unwrap()], &[includes.to_str().unwrap()])
                .expect("Failed to compile proto files");
        } else {
            println!("Warning: proto file not found at {}", proto_path.display());
        }
    }
}
