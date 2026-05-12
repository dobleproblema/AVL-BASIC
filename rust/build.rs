fn main() {
    println!("cargo:rerun-if-changed=windows.rc");
    println!("cargo:rerun-if-changed=assets/avl-basic.ico");

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_resource::compile("windows.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
