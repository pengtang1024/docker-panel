//! 日志时间戳游标：RFC3339Nano 前缀解析 / 剥离 / 纳秒过滤（纯逻辑）
//!
//! `json-file` 驱动的**所有跟随流**都开 `timestamps`：正常段每行只切首个空格
//! 剥前缀（成本优化，不解析），每批投递时只解析**最后一行**的 token 推进游标；
//! 只有断裂后的重连路径才带 `since=游标秒`，回放段逐行解析做纳秒过滤。
//! 首次建流与 `Clear` 永不带 `since`，日志面板与「复制日志」保持原文（无前缀）。
//!
//! 为什么必须解析而不能字符串比较：Docker 的时间戳是 RFC3339Nano，**尾随零省略**、
//! 小数位数不固定（`.1Z` 与 `.09Z` 的字典序与真实时间相反），只能比较 `(秒, 纳秒)`。

use std::collections::VecDeque;

/// 单次回放保留上限（行）。
///
/// 与日志窗口上限 `LOG_ROWS_MAX` 对齐：超过 2000 行的历史即使全部补回，
/// 旧行也会立刻被窗口分块裁剪掉，没有展示价值。因此回放只保留**最新的一段**，
/// 更早的行主动丢弃（宁缺旧行，也不让一次重连把行模型撑爆）。
pub const REPLAY_MAX: usize = 2000;

/// 回放「追平实时」判定容差（秒）。
///
/// 时间戳由本机 Docker 守护进程在读取容器输出时打上，和客户端墙钟同源；
/// 留 2s 容差覆盖调度抖动。回放中出现「接近当前时刻」的行即视为已追平。
pub const CATCHUP_SLACK_SECS: i64 = 2;

/// 纳秒级日志游标（严格大于它才接受，用于过滤 `since` 秒级重放的重叠段）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LogCursor {
    /// UNIX 秒
    pub sec: i64,
    /// 秒内纳秒（0..1_000_000_000）
    pub nanos: u32,
}

impl LogCursor {
    /// Docker `since` 只接受 i32 秒（含该秒全部行），同秒重叠由纳秒游标二次过滤。
    pub fn since_secs(self) -> i32 {
        self.sec.clamp(0, i32::MAX as i64) as i32
    }
}

/// 一行经过游标判定后的去向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilteredLine<'a> {
    /// 时间戳晚于游标：保留（已剥掉时间戳前缀）
    Keep { text: &'a str, ts: LogCursor },
    /// 时间戳不晚于游标：`since` 秒级重放的重叠段，丢弃
    Duplicate,
    /// 没有可识别的时间戳前缀：原样保留（保守起见不丢任何正文）
    Raw { text: &'a str },
}

/// 解析行首 RFC3339Nano 时间戳并剥离（**只切第一个空格**，正文可含空格）。
///
/// 返回 `None` 表示首个空格之前不是合法时间戳（调用方应保留整行原文）。
pub fn parse_prefix(line: &str) -> Option<(LogCursor, &str)> {
    let (head, rest) = line.split_once(' ')?;
    Some((parse_ts_token(head)?, rest))
}

/// 只剥前缀、**不解析**（正常运行段的成本优化：每行只切第一个空格）。
///
/// 返回 `(正文, 行首 token)`；没有空格时正文为整行、token 为 `None`。
/// 调用方只在批量投递时对最后一条的 token 调用 [`parse_ts_token`] 推进游标。
///
/// 行首 token 必须**形状**像 RFC3339Nano（`...Z`）才剥离，否则整行原样保留：
/// 防止空消息行（Docker 输出 `时间戳 + 空格`，trim 后只剩时间戳）把时间戳当正文。
pub fn split_prefix(line: &str) -> (&str, Option<&str>) {
    match line.split_once(' ') {
        Some((head, rest)) if looks_like_ts(head) => (rest, Some(head)),
        _ if looks_like_ts(line) => ("", Some(line)),
        _ => (line, None),
    }
}

/// 行首 token 是否「形状」像 RFC3339Nano（只做字节结构检查，**不解析数字**）
fn looks_like_ts(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 20
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && b[b.len() - 1] == b'Z'
}

/// 纯函数判定：`line` 是否在游标之后、正文是什么。`cursor=None` 时不做过滤。
pub fn classify_line(line: &str, cursor: Option<LogCursor>) -> FilteredLine<'_> {
    match parse_prefix(line) {
        Some((ts, text)) => match cursor {
            Some(c) if ts <= c => FilteredLine::Duplicate,
            _ => FilteredLine::Keep { text, ts },
        },
        None => FilteredLine::Raw { text: line },
    }
}

/// 回放是否已追平实时（用于把缓冲的最新一段投递出去）
pub fn is_caught_up(ts: LogCursor, now_sec: i64) -> bool {
    ts.sec >= now_sec.saturating_sub(CATCHUP_SLACK_SECS)
}

/// 回放行缓冲：容量固定为 [`REPLAY_MAX`]，超出丢最旧、保留最新一段。
#[derive(Debug, Default)]
pub struct ReplayBuffer {
    lines: VecDeque<String>,
}

impl ReplayBuffer {
    /// 压入一条（已剥时间戳的）正文；超过上限时挤掉最旧一行
    pub fn push(&mut self, line: String) {
        if self.lines.len() >= REPLAY_MAX {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    /// 取走全部缓冲（按时间顺序，旧 → 新）
    pub fn take(&mut self) -> Vec<String> {
        self.lines.drain(..).collect()
    }

    /// 缓冲区是否为空
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// 解析 `YYYY-MM-DDTHH:MM:SS[.fraction]Z`（Docker 统一 UTC，小数最多 9 位）
///
/// 正常段每批只调用一次（最后一行）；回放段逐行调用做纳秒过滤。
pub fn parse_ts_token(s: &str) -> Option<LogCursor> {
    let b = s.strip_suffix('Z')?.as_bytes();
    if b.len() < 19
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let num = |at: usize, len: usize| -> Option<i64> {
        let mut v = 0i64;
        for &c in &b[at..at + len] {
            if !c.is_ascii_digit() {
                return None;
            }
            v = v * 10 + (c - b'0') as i64;
        }
        Some(v)
    };
    let (y, m, d) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (hh, mm, ss) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 59 {
        return None;
    }
    let nanos = if b.len() == 19 {
        0
    } else {
        // 只接受 `.digits`，且小数最多 9 位（RFC3339Nano 上限）
        if b[19] != b'.' || b.len() < 21 || b.len() > 29 {
            return None;
        }
        let frac = &b[20..];
        if !frac.iter().all(u8::is_ascii_digit) {
            return None;
        }
        let mut n: u32 = 0;
        for &c in frac {
            n = n * 10 + (c - b'0') as u32;
        }
        // 尾随零省略 → 右补零到 9 位（`.5` = 500_000_000ns）
        for _ in frac.len()..9 {
            n *= 10;
        }
        n
    };
    let sec = days_from_civil(y, m, d) * 86_400 + hh * 3_600 + mm * 60 + ss;
    Some(LogCursor { sec, nanos })
}

/// 公历日期 → 1970-01-01 起的天数（Howard Hinnant 算法，纯整数）
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nano_prefix_and_body() {
        let (c, body) = parse_prefix("2026-11-04T07:12:33.123456789Z hello world").unwrap();
        assert_eq!(body, "hello world", "只切第一个空格，正文保留空格");
        assert_eq!(c.nanos, 123_456_789);
        assert_eq!(c.since_secs(), 1_793_776_353);
    }

    #[test]
    fn trailing_zeros_are_padded_to_nanos() {
        // `.1` 与 `.09`：字典序相反，必须按数值比较
        let a = parse_prefix("2026-11-04T07:12:33.1Z a").unwrap().0;
        let b = parse_prefix("2026-11-04T07:12:33.09Z b").unwrap().0;
        assert_eq!(a.nanos, 100_000_000);
        assert_eq!(b.nanos, 90_000_000);
        assert!(a > b, "纳秒比较：.1 > .09（字符串比较会得出相反结论）");
        // 无小数
        let n = parse_prefix("1970-01-01T00:00:00Z x").unwrap().0;
        assert_eq!((n.sec, n.nanos), (0, 0));
    }

    #[test]
    fn body_may_start_with_digits() {
        let (_, body) = parse_prefix("2026-11-04T07:12:33.12Z 123 abc").unwrap();
        assert_eq!(body, "123 abc", "剥前缀不得误伤数字开头的正文");
    }

    #[test]
    fn same_second_different_nanos_is_strictly_after() {
        let c = parse_prefix("2026-11-04T07:12:33.1Z a").unwrap().0;
        assert_eq!(
            classify_line("2026-11-04T07:12:33.2Z b", Some(c)),
            FilteredLine::Keep {
                text: "b",
                ts: parse_prefix("2026-11-04T07:12:33.2Z b").unwrap().0
            }
        );
        assert_eq!(
            classify_line("2026-11-04T07:12:33.1Z a", Some(c)),
            FilteredLine::Duplicate,
            "游标本身的行属于重放重叠段"
        );
        assert_eq!(
            classify_line("2026-11-04T07:12:32.9Z old", Some(c)),
            FilteredLine::Duplicate
        );
    }

    #[test]
    fn cursor_advances_one_line_at_a_time() {
        let mut c = None;
        for line in [
            "2026-11-04T07:12:33.1Z one",
            "2026-11-04T07:12:33.2Z two",
            "2026-11-04T07:12:34Z three",
        ] {
            match classify_line(line, c) {
                FilteredLine::Keep { ts, .. } => c = Some(ts),
                other => panic!("应保留新行: {other:?}"),
            }
        }
        assert_eq!(c.unwrap().sec, 1_793_776_354);
        // 游标推进后，更早的行全部判重
        assert_eq!(
            classify_line("2026-11-04T07:12:33.1Z one", c),
            FilteredLine::Duplicate
        );
    }

    #[test]
    fn invalid_timestamp_falls_back_to_raw() {
        assert_eq!(
            classify_line("plain log without ts", None),
            FilteredLine::Raw {
                text: "plain log without ts"
            }
        );
        // 月份 / 时区非法：整行保留，不剥
        assert_eq!(
            classify_line(
                "2026-13-99T99:99:99Z boom",
                Some(LogCursor { sec: 0, nanos: 0 })
            ),
            FilteredLine::Raw {
                text: "2026-13-99T99:99:99Z boom"
            }
        );
        // 小数超过 9 位 / 无空格：都不解析
        assert!(parse_prefix("2026-11-04T07:12:33.1234567891Z x").is_none());
        assert!(parse_prefix("2026-11-04T07:12:33Z").is_none());
    }

    #[test]
    fn replay_buffer_keeps_newest_segment() {
        let mut buf = ReplayBuffer::default();
        for i in 0..(REPLAY_MAX + 5) {
            buf.push(format!("line-{i}"));
        }
        let out = buf.take();
        assert_eq!(out.len(), REPLAY_MAX);
        assert_eq!(out.first().unwrap(), "line-5", "超出上限时丢最旧");
        assert_eq!(out.last().unwrap(), &format!("line-{}", REPLAY_MAX + 4));
        assert!(buf.is_empty());
    }

    #[test]
    fn split_prefix_strips_without_parsing() {
        assert_eq!(
            split_prefix("2026-11-04T07:12:33.5Z hello world"),
            ("hello world", Some("2026-11-04T07:12:33.5Z"))
        );
        // 空消息行：trim 后只剩时间戳 → 正文为空，时间戳仍可推进游标
        assert_eq!(
            split_prefix("2026-11-04T07:12:33.5Z"),
            ("", Some("2026-11-04T07:12:33.5Z"))
        );
        // 形状不像时间戳：整行原样保留，不误剥正文
        assert_eq!(split_prefix("plain log"), ("plain log", None));
    }

    #[test]
    fn catchup_uses_two_second_slack() {
        assert!(is_caught_up(LogCursor { sec: 100, nanos: 0 }, 101));
        assert!(is_caught_up(LogCursor { sec: 98, nanos: 0 }, 100));
        assert!(!is_caught_up(LogCursor { sec: 97, nanos: 0 }, 100));
    }
}
