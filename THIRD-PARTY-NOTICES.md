# 第三方组件与许可证通知

本文件适用于 docker-panel 的**二进制发行物**（`docker-panel`、`docker-panel_*.deb`）。
仓库自有代码的许可证见 [`LICENSE`](LICENSE)（MIT），许可选择记录见 [`LICENSING.md`](LICENSING.md)。

## 1. 本项目自有代码

**MIT**。全文见 [`LICENSE`](LICENSE)，覆盖 `src/`、`ui/`、`translations/`、`docs/`、`scripts/`、`packaging/` 等原创文件。

## 2. UI 框架：Slint 1.18.1

- 上游许可：`GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`
- 本项目选择：**`LicenseRef-Slint-Royalty-free-2.0`**
- 许可全文：[`licenses/Slint-Royalty-free-2.0.md`](licenses/Slint-Royalty-free-2.0.md)
- 署名（许可 §2(b)）：README 顶部 “Made with Slint” 徽章 + GitHub Release 说明页
- 静态链入二进制：`slint`、`i-slint-core`、`i-slint-common`、`i-slint-backend-selector`、`i-slint-backend-winit`、`i-slint-renderer-software`
- 仅编译期使用（不进入二进制）：`slint-macros`、`i-slint-core-macros`、`i-slint-compiler`、`slint-build`

> `Cargo.lock` 还锁定 `i-slint-backend-linuxkms`、`i-slint-backend-testing`、`i-slint-renderer-skia`。它们未在 Linux 目标启用，不会编译进发行物；许可与本项相同。

## 3. Slint Material Components 1.1.0

- 位置：`material-1.1.0/`（编译进 UI）
- 许可证：**MIT**，Copyright (c) 2026 SixtyFPS GmbH
- 全文：[`licenses/MIT-SixtyFPS.md`](licenses/MIT-SixtyFPS.md)

## 4. 其他第三方组件（按许可证分组）

> 下表列出直接依赖与主要传递依赖。**完整依赖图以 `Cargo.lock` 为准**（547 个锁定包，Linux 目标 345 个 normal 依赖）。

| 许可证 | 组件（版本） | 全文 |
|--------|--------------|------|
| **Apache-2.0** | `bollard` 0.21.1（Docker API）、`bollard-stubs` 1.53.1-rc.29.3.1、`winit` 0.30.13、`ab_glyph` 0.2.32、`ab_glyph_rasterizer` 0.1.10、`owned_ttf_parser` 0.25.1、`gethostname` 1.1.0、`unicode-linebreak` 0.1.5 | [`licenses/Apache-2.0.txt`](licenses/Apache-2.0.txt) |
| **BSD-3-Clause** | `tiny-skia` 0.11.4 / 0.12.0、`tiny-skia-path` 0.11.4 / 0.12.0 | [`licenses/BSD-3-Clause.txt`](licenses/BSD-3-Clause.txt) |
| **BSD-2-Clause** | `arrayref` 0.3.9 | [`licenses/BSD-2-Clause.txt`](licenses/BSD-2-Clause.txt) |
| **ISC** | `libloading` 0.8.9 | [`licenses/ISC.txt`](licenses/ISC.txt) |
| **Zlib** | `foldhash` 0.1.5 / 0.2.0、`slotmap` 1.1.1、`zlib-rs` 0.6.8 | [`licenses/Zlib.txt`](licenses/Zlib.txt) |
| **Unicode-3.0** | ICU4X 组件：`icu_*`、`fixed_decimal`、`litemap`、`potential_utf`、`tinystr`、`writeable`、`yoke*`、`zerofrom*`、`zerotrie`、`zerovec*`（共 25 个） | [`licenses/Unicode-3.0.txt`](licenses/Unicode-3.0.txt) |

### 双许可组件（本项目选择的分支）

| 上游表达式 | 代表组件 | 本项目选择 |
|------------|----------|------------|
| `MIT OR Apache-2.0` | `tokio` 1.53.3、`hyper` 1.12.0、`futures` 0.3.34、`serde` 1.0.229、`zbus` 5.19.0、`image` 0.25.10、`usvg` / `resvg` 0.48.1、`parley` 0.11.1、`swash` 0.2.10、`wayland-*` 0.31.x、`x11rb` 0.13.2、`sha2` 0.10.9、`digest` 0.10.7、`block-buffer` 0.10.4、`crypto-common` 0.1.7、`generic-array` 0.14.7、`typenum` 1.20.1、`cpufeatures` 0.2.17 等 222 个 | MIT |
| `Apache-2.0 OR MIT` | `indexmap` 2.14.2、`pin-project` 1.1.13、`polling` 3.11.0 等 40 个 | MIT |
| `Unlicense OR MIT` | `memchr` 2.8.3、`byteorder-lite` 0.1.0 | MIT |
| `CC0-1.0 OR Apache-2.0` | `imgref` 1.12.3 | Apache-2.0（规避 CC0 的 OSI 认定问题） |
| `0BSD OR MIT OR Apache-2.0` | `adler2` 2.0.1 | MIT |
| `BSD-3-Clause OR Apache-2.0` | `moxcms` 0.8.1、`pxfm` 0.1.30 | Apache-2.0 |
| `Apache-2.0 OR BSL-1.0` | `ryu` 1.0.23 | Apache-2.0（BSL-1.0 是 Boost 许可，非 BUSL） |
| `MIT OR Zlib OR Apache-2.0` | `miniz_oxide` 0.8.9 / 0.9.1 | MIT |
| `Zlib OR Apache-2.0 OR MIT` | `bytemuck` 1.25.2、`tinyvec` 1.13.3 | MIT |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | `rustix` 0.38.44 / 1.1.5、`linux-raw-sys` | MIT / Apache-2.0 |
| `BSD-2-Clause OR Apache-2.0 OR MIT` | `zerocopy` 0.8.63 | MIT |
| `MIT OR Apache-2.0 OR Zlib` | `raw-window-handle` 0.6.2、`xkeysym` 0.2.1、`zune-core` / `zune-jpeg` | MIT |

### 未构建的可选组件

`r-efi` 5.3.0 / 6.0.0 的上游表达式含 `LGPL-2.1-or-later` 分支，但它是 UEFI 目标依赖，**不参与 Linux/macOS/Windows 桌面构建**；本项目未选择 LGPL 分支。

## 5. 完整清单与复现方式

许可全文在 deb 的 `/usr/share/doc/docker-panel/licenses/`，或 Release 附件 `licenses.tar.gz`。

```bash
cargo metadata --format-version 1   # 每个包的 license / license_file 字段
cargo tree --target x86_64-unknown-linux-gnu -e normal   # 实际参与构建的依赖树
```

Linux 目标实测：345 个 normal 依赖，其中静态链入的 copyleft 选项组件只有 Slint 一组，已按 Royalty-free 2.0 合规；其余均为 MIT / Apache-2.0 / BSD / ISC / Zlib / Unicode 等宽松许可。

## 6. 维护规则

- 升级或新增依赖后，重新核对本表与 `Cargo.lock`；
- 引入 GPL / AGPL / LGPL / SSPL / BUSL / Commons Clause 前必须停下评估，不得直接合入；
- 上游提供 `NOTICE` 文件时，必须把其内容转抄到本文件（当前 Apache-2.0 组件均未提供 NOTICE）。

---

最后更新：2026-10-28（docker-panel 0.5.0 + 自动更新未发布改动）
