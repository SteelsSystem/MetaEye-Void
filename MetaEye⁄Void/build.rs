use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=proto/needle_discord_metacog.proto");
    println!("cargo:rerun-if-changed=contracts/openapi.yaml");
    println!("cargo:rerun-if-changed=contracts/schema.sql");

    let proto_path = Path::new("proto/needle_discord_metacog.proto");
    if proto_path.exists() && std::env::var("PROTOC").is_ok() {
        tonic_build::configure()
            .build_server(true)
            .build_client(true)
            .compile_protos(&[proto_path], &[Path::new("proto")])
            .expect("Failed to compile Protobuf specification");
    }
}
