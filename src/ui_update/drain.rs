//! `DrainResult`：一次批量 drain 的聚合结果（从 ui_update.rs 拆出，满足 R2）。
//!
//! ## 为什么是「位掩码 + 声明宏」而不是 derive 宏
//!
//! 旧实现有三份手抄清单（`has_ui_changes` / `logs_only` / `merge`），每个 UI 标志都要
//! 再抄一遍：新增一个标志 = 三处各改一次，漏一处就是静默 bug（漏在 `merge` → 多批合并
//! 时丢掉刷新；漏在 `logs_only` → 状态栏被 250ms 的宽松窗口拖慢）。
//!
//! 这里把「有没有变化」压进一个 u32 位掩码：
//! - 布尔标志用位表示 → `has_ui_changes` = `flags != 0`、`logs_only` = `flags == LOGS_BIT`；
//! - 带载荷的标志（`Option`）单独存值，扫描表达式由声明宏生成；
//! - `merge` = 位掩码 OR + 每个载荷 last-wins，同样由宏生成。
//!
//! 选它而不是 derive 宏：
//! 1. derive 宏要新增 proc-macro crate（本项目零额外依赖），收益只是少抄一次字段名；
//! 2. derive 仍要求把「字段 → 语义」写进属性（哪些是纯日志标志、哪些是载荷），
//!    编译器能挡住的范围并不比声明宏更大；
//! 3. 声明宏在 crate 内：字段清单、位定义、三份清单的扫描、一致性测试由同一份声明生成，
//!    `cargo test` 即可验证「字段清单 ↔ 位定义」没有漂移。
//!
//! 新增一个 UI 标志的步骤：在下面 `flags:` 里加一行（位号重复会被生成的测试断言，
//! 第 33 个会在编译期移位溢出报错）；带载荷时在 `options:` 里再加一行。
//! 编译器能挡住：旧代码残留的 `res.new_flag = true`（字段已不存在 → E0609）、
//! 位号越界（`1u32 << 32` 常量求值溢出）、三份清单不同步（生成的测试逐位核对）。

/// 声明一次，生成：`UiFlag` 枚举、结构体字段、三份清单与一致性测试。
macro_rules! drain_result {
    (
        flags: {
            $( $( #[$fattr:meta] )* $flag:ident = $bit:expr ; )*
        }
        options: {
            $( $( #[$oattr:meta] )* $opt:ident : $oty:ty ; )*
        }
    ) => {
        /// 一个 UI 变化标志。判别值即位掩码，`DrainResult` 用它记录本批发生了什么。
        #[repr(u32)]
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum UiFlag {
            $( $( #[$fattr] )* $flag = 1u32 << $bit, )*
        }

        impl UiFlag {
            /// 全部标志（一致性测试遍历用）
            #[cfg(test)]
            pub const ALL: &[UiFlag] = &[ $( UiFlag::$flag ),* ];

            /// 位掩码值（判别值即 1 << 位号）
            pub const fn bit(self) -> u32 {
                self as u32
            }
        }

        /// 一次批量 drain 的聚合结果。
        ///
        /// 布尔标志只能经 [`DrainResult::mark`] 置位（字段不公开，杜绝绕过掩码的直写）；
        /// `Option` 载荷直接赋值即可，扫描与合并由宏生成。
        #[derive(Default)]
        pub struct DrainResult {
            /// 本批实际处理的 Msg 数（0 = 只是被 recv 超时叫醒）
            pub processed: usize,
            /// 还有积压（本批达到上限被截断）
            pub has_more: bool,
            /// AppState.shutdown 已置位
            pub shutdown: bool,
            /// 已置位的 UI 标志位
            flags: u32,
            $(
                $( #[$oattr] )*
                pub $opt: Option<$oty>,
            )*
        }

        impl DrainResult {
            /// 置位一个 UI 变化标志
            pub fn mark(&mut self, f: UiFlag) {
                self.flags |= f.bit();
            }

            /// 查询一个 UI 变化标志
            pub fn is_set(&self, f: UiFlag) -> bool {
                self.flags & f.bit() != 0
            }

            /// 本批是否产生了需要落到 UI 的变化。
            ///
            /// 位掩码非零，或任一载荷到达即为真——两项都是宏生成的全量扫描，
            /// 新增字段不需要改这里。
            pub fn has_ui_changes(&self) -> bool {
                self.flags != 0 $( || self.$opt.is_some() )*
            }

            /// 本批是否**只有**日志行变化。
            ///
            /// 日志行追加只影响日志视口，可以用比常规 UI 更宽的节流窗口；
            /// 任何其它变化（列表 / 抽屉 / 状态栏 / 弹窗）都必须退回 120ms。
            pub fn logs_only(&self) -> bool {
                self.flags == UiFlag::LogsChanged.bit() $( && self.$opt.is_none() )*
            }

            /// 把下一批结果并入累计结果（UI 刷新按时间窗口节流时会连续 drain 多批）
            pub fn merge(&mut self, o: DrainResult) {
                self.processed += o.processed;
                self.has_more = o.has_more;
                self.shutdown |= o.shutdown;
                self.flags |= o.flags;
                $(
                    if o.$opt.is_some() {
                        self.$opt = o.$opt;
                    }
                )*
            }
        }

        /// 字段清单 ↔ 位定义的一致性测试（宏生成，新增字段自动纳入）
        #[cfg(test)]
        mod generated_tests {
            use super::*;

            /// 每个标志都必须被 `has_ui_changes` 看到，且只有日志位成立 `logs_only`
            #[test]
            fn every_flag_is_wired_into_both_lists() {
                for &f in UiFlag::ALL {
                    let mut r = DrainResult::default();
                    r.mark(f);
                    assert!(r.has_ui_changes(), "{f:?} 未接入 has_ui_changes");
                    assert_eq!(
                        r.logs_only(),
                        f == UiFlag::LogsChanged,
                        "{f:?} 的 logs_only 判定错误"
                    );
                }
            }

            /// 位号必须互不重复（重复 = 两个标志共用一个位）
            #[test]
            fn flags_use_distinct_bits() {
                let mut seen = 0u32;
                $(
                    assert!(
                        (UiFlag::$flag.bit() & seen) == 0,
                        "{:?} 的位与已有标志重复",
                        UiFlag::$flag
                    );
                    seen |= UiFlag::$flag.bit();
                )*
            }

            /// 任一载荷到达都算 UI 变化，且不算「纯日志」
            #[test]
            fn option_payloads_count_as_ui_changes() {
                $(
                    let mut r = DrainResult::default();
                    r.$opt = Some(<$oty as Default>::default());
                    assert!(r.has_ui_changes(), "{} 未接入 has_ui_changes", stringify!($opt));
                    assert!(!r.logs_only(), "{} 不应被视为纯日志", stringify!($opt));
                )*
            }

            /// merge 必须保留位掩码与每个载荷（漏合并 = 多批合并时丢刷新）
            #[test]
            fn merge_keeps_flags_and_payloads() {
                $(
                    let mut a = DrainResult::default();
                    let mut b = DrainResult::default();
                    b.$opt = Some(<$oty as Default>::default());
                    a.merge(b);
                    assert!(a.$opt.is_some(), "merge 丢了 {}", stringify!($opt));
                )*
                let mut a = DrainResult::default();
                let mut b = DrainResult::default();
                for &f in UiFlag::ALL {
                    b.mark(f);
                }
                a.merge(b);
                for &f in UiFlag::ALL {
                    assert!(a.is_set(f), "merge 丢了 {f:?}");
                }
            }
        }
    };
}

drain_result! {
    flags: {
        /// 容器台账行变化（列表刷新 / stats 采样 / 选中态变化）
        ContainersChanged = 0;
        /// 镜像行变化
        ImagesChanged = 1;
        /// 网络行变化
        NetworksChanged = 2;
        /// 卷行变化（含 30s 磁盘占用回填）
        VolumesChanged = 3;
        /// 抽屉内容变化（详情 / 环境变量 / 容器 stats）
        DrawerChanged = 4;
        /// 日志行列表变化
        LogsChanged = 5;
        /// 本批出现过操作结果 toast（2s 后恢复 Ready）
        Toast = 6;
        /// 关闭拉取弹窗
        ClosePull = 7;
        /// 复位拉取按钮的 busy 态
        ResetPullBusy = 8;
        /// 需要立即触发一次列表刷新
        RefreshNow = 9;
        /// 更新检查完成（成功/失败都置位；manual 失败才显示到弹窗）
        UpdateCheckDone = 10;
        /// 更新下载/安装进度变化
        UpdateProgress = 11;
        /// 更新安装完成（成功或失败）
        UpdateDone = 12;
    }
    options: {
        /// 列表刷新的连通性结果（None = 本批没有列表结果）。
        ///
        /// 连通性**只**由列表刷新结果决定：操作类消息（启停/拉取/清理）无论成败都不置位，
        /// 否则失败场景会被 `Some(true)` 掩盖，红点显示不出来。
        docker_ok: bool;
        /// 需要写入 status_text 的文本（toast / pull 进度）
        status_text: String;
        /// 日志流状态提示（重连中 / 已恢复 / 已停止）——独立通道，不覆盖状态栏
        log_status: String;
        /// Docker 守护进程版本文案（取不到时为占位符 `—`）
        docker_version: String;
        /// stats 采样是否已过期（连续多轮无样本；只在跃迁时到达）
        stats_stale: bool;
        /// 磁盘占用采样是否已过期（`/system/df` 连续多轮失败；只在跃迁时到达）
        disk_stale: bool;
    }
}
