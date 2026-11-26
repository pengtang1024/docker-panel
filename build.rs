fn main() {
    println!("cargo:rerun-if-changed=translations");
    println!("cargo:rerun-if-changed=material-1.1.0");

    let manifest_dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let material_path = manifest_dir.join("material-1.1.0/material.slint");

    // i18n 目录表：只把真实存在的 .po 编译进二进制。其它 worker 新建的
    // es_ES / fr_FR / de_DE / ru_RU / pt_PT .po 一旦落地，下一次 cargo 检查
    // 会因为 rerun-if-changed=translations 自动重新生成，无需改 build.rs。
    let locales = ["zh_CN", "es_ES", "fr_FR", "de_DE", "ru_RU", "pt_PT"];
    let mut catalogs = String::from(
        "pub static CATALOGS: &[(&str, &str)] = &[
",
    );
    for locale in locales {
        let po = manifest_dir
            .join("translations")
            .join(locale)
            .join("LC_MESSAGES")
            .join("docker-panel.po");
        let entry = if po.is_file() {
            format!(
                "    ({:?}, include_str!({:?})),\n",
                locale,
                po.to_string_lossy()
            )
        } else {
            format!("    ({:?}, \"\"),\n", locale)
        };
        catalogs.push_str(&entry);
    }
    catalogs.push_str("];\n");
    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    std::fs::write(
        std::path::Path::new(&out_dir).join("i18n_catalogs.rs"),
        catalogs,
    )
    .expect("write i18n catalogs");

    let config = slint_build::CompilerConfiguration::new()
        .with_debug_info(true)
        .with_default_translation_context(slint_build::DefaultTranslationContext::None)
        .with_bundled_translations("translations")
        .with_library_paths(std::collections::HashMap::from([(
            "material".to_string(),
            material_path,
        )]));
    slint_build::compile_with_config("ui/app.slint", config).expect("slint compile failed");
}
