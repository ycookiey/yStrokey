fn main() {
    // exeとトレイのアイコンをリソースとしてexeに埋め込む
    println!("cargo:rerun-if-changed=assets");
    embed_resource::compile("assets/app.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
