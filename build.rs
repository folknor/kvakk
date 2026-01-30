fn main() {
    // Tauri build
    tauri_build::build();

    // Protobuf compilation
    let mut config = prost_build::Config::new();
    config.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");

    config
        .compile_protos(
            &[
                "src-tauri/src/proto_src/device_to_device_messages.proto",
                "src-tauri/src/proto_src/offline_wire_formats.proto",
                "src-tauri/src/proto_src/securegcm.proto",
                "src-tauri/src/proto_src/securemessage.proto",
                "src-tauri/src/proto_src/ukey.proto",
                "src-tauri/src/proto_src/wire_format.proto",
            ],
            &["src-tauri/src/proto_src"],
        )
        .unwrap();
}
