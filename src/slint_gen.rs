//! Slint 生成的类型集中入口，确保 main.rs 和 callbacks.rs 共享同一套类型
//!
//! Slint 的 `include_modules!()` 会每次展开都生成全新的 MainWindow / AppActions / ContainerRow 等类型，
//! 如果在多个文件里分别调用它们会变成不同的 Rust 类型导致 `mismatched types`。
//! 解决方案：只在本模块调一次 `include_modules!()`，其他模块通过 `use crate::slint_gen::*` 导入。

slint::include_modules!();
