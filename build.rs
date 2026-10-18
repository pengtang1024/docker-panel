fn main() {
    println!("cargo:rerun-if-changed=translations");
    println!("cargo:rerun-if-changed=material-1.1.0");

    let material_path = std::path::Path::new(&std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("material-1.1.0/material.slint");

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
