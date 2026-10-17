fn main() {
    // 翻译配置：translations/<lang>/LC_MESSAGES/docker-panel.po（gettext 格式）
    // 编译期嵌入二进制（单二进制交付），运行时按系统 locale 通过
    // slint::select_bundled_translation() 选择；未命中语言回落到英文源串。
    println!("cargo:rerun-if-changed=translations");
    let config = slint_build::CompilerConfiguration::new()
        .with_debug_info(true)
        .with_default_translation_context(slint_build::DefaultTranslationContext::None)
        .with_bundled_translations("translations");
    slint_build::compile_with_config("ui/app.slint", config).expect("slint compile failed");
}
