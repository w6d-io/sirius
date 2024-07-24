pub fn main() {
    let proto_files = ["./proto/permission.proto"];

    tonic_build::configure()
        .type_attribute("Input", "#[derive(serde::Deserialize, serde::Serialize)]")
        .build_server(true)
        .compile(&proto_files, &["."])
        .unwrap_or_else(|e| panic!("protobuf compile error: {e}"));
    println!("cargo:rerun-if-changed={proto_files:?}");
}
