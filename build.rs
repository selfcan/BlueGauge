fn main() {
    load_logo();
}

fn load_logo() {
    embed_resource::compile("assets/logo.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
