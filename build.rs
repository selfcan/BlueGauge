fn main() {
    embed_resource::compile("assets/BlueGauge.exe.manifest.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
