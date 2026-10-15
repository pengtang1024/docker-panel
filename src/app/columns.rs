//! 表格列定义与排序比较器：容器列 CtlCol、镜像列 ImgCol

use super::*;

/// 容器表的可选列（顺序即默认显示顺序）
#[derive(PartialEq, Eq, Clone, Copy)]
pub(super) enum CtlCol {
    Id,
    Name,
    Image,
    State,
    Uptime,
    Ports,
    Cpu,
    Mem,
    Disk,
    Net,
    Actions,
}

impl CtlCol {
    /// 全部列（默认显示顺序）
    pub(super) const ALL: [CtlCol; 11] = [
        CtlCol::Id,
        CtlCol::Name,
        CtlCol::Image,
        CtlCol::State,
        CtlCol::Uptime,
        CtlCol::Ports,
        CtlCol::Cpu,
        CtlCol::Mem,
        CtlCol::Disk,
        CtlCol::Net,
        CtlCol::Actions,
    ];

    /// 表头文案
    pub(super) fn title(self) -> &'static str {
        match self {
            CtlCol::Id => "ID",
            CtlCol::Name => "名称",
            CtlCol::Image => "镜像",
            CtlCol::State => "状态",
            CtlCol::Uptime => "运行时长",
            CtlCol::Ports => "端口",
            CtlCol::Cpu => "CPU",
            CtlCol::Mem => "内存",
            CtlCol::Disk => "磁盘",
            CtlCol::Net => "网络",
            CtlCol::Actions => "操作",
        }
    }

    /// 列菜单里的复选框文案
    pub(super) fn label(self) -> &'static str {
        match self {
            CtlCol::Id => "短 ID",
            other => other.title(),
        }
    }

    /// 是否参与排序
    pub(super) fn sortable(self) -> bool {
        self != CtlCol::Actions
    }

    /// 数值列（右对齐）
    pub(super) fn right(self) -> bool {
        matches!(self, CtlCol::Cpu | CtlCol::Mem | CtlCol::Disk | CtlCol::Net)
    }

    /// 列的（初始宽度, 最小宽度），column() 与 width() 共用一份宽度表
    fn dims(self) -> (f32, f32) {
        match self {
            CtlCol::Id => (95.0, 80.0),
            CtlCol::Name => (150.0, 100.0),
            CtlCol::Image => (180.0, 120.0),
            CtlCol::State => (80.0, 70.0),
            CtlCol::Uptime => (130.0, 90.0),
            CtlCol::Ports => (200.0, 120.0),
            CtlCol::Cpu => (75.0, 65.0),
            CtlCol::Mem => (140.0, 100.0),
            CtlCol::Disk => (130.0, 100.0),
            CtlCol::Net => (140.0, 100.0),
            CtlCol::Actions => (44.0, 44.0),
        }
    }

    /// 表格列定义（last 用 remainder 占满剩余宽度）
    pub(super) fn column(self, last: bool) -> Column {
        let (init, min) = self.dims();
        let col = if last {
            Column::remainder()
        } else {
            Column::initial(init)
        };
        col.at_least(min).resizable(true)
    }

    /// 参与排序的列的期望宽度合计（用于设置表格最小宽度）
    pub(super) fn width(self) -> f32 {
        self.dims().0
    }
}

/// 镜像表的可选列
#[derive(PartialEq, Eq, Clone, Copy)]
pub(super) enum ImgCol {
    Name,
    Tag,
    Size,
    Created,
    Refs,
    Actions,
}

impl ImgCol {
    /// 全部列（默认显示顺序）
    pub(super) const ALL: [ImgCol; 6] = [
        ImgCol::Name,
        ImgCol::Tag,
        ImgCol::Size,
        ImgCol::Created,
        ImgCol::Refs,
        ImgCol::Actions,
    ];

    /// 表头文案
    pub(super) fn title(self) -> &'static str {
        match self {
            ImgCol::Name => "名称",
            ImgCol::Tag => "Tag",
            ImgCol::Size => "大小",
            ImgCol::Created => "创建时间",
            ImgCol::Refs => "关联容器",
            ImgCol::Actions => "操作",
        }
    }

    /// 是否参与排序
    pub(super) fn sortable(self) -> bool {
        self != ImgCol::Actions
    }

    /// 数值列（右对齐）
    pub(super) fn right(self) -> bool {
        matches!(self, ImgCol::Size | ImgCol::Refs)
    }

    /// 列的（初始宽度, 最小宽度），column() 与 width() 共用一份宽度表
    fn dims(self) -> (f32, f32) {
        match self {
            ImgCol::Name => (240.0, 150.0),
            ImgCol::Tag => (130.0, 80.0),
            ImgCol::Size => (150.0, 110.0),
            ImgCol::Created => (140.0, 100.0),
            ImgCol::Refs => (110.0, 80.0),
            ImgCol::Actions => (60.0, 60.0),
        }
    }

    /// 表格列定义
    pub(super) fn column(self, last: bool) -> Column {
        let (init, min) = self.dims();
        let col = if last {
            Column::remainder()
        } else {
            Column::initial(init)
        };
        col.at_least(min).resizable(true)
    }

    /// 列宽合计（用于设置表格最小宽度）
    pub(super) fn width(self) -> f32 {
        self.dims().0
    }
}

/// 表格渲染结果（容器/镜像两张表共用；渲染结束后由上层统一应用，规避 egui 借用冲突）
pub(super) struct TableOutcome<C> {
    /// 全选框切换结果
    pub(super) select_all: Option<bool>,
    /// 被点击的排序表头
    pub(super) sort_click: Option<C>,
    /// 单击/复选框切换的行 key（容器为 ID，镜像为 "名称:tag"）
    pub(super) toggles: Vec<String>,
    /// 行内按钮/右键菜单产生的动作
    pub(super) actions: Vec<RowAction>,
}

impl<C> Default for TableOutcome<C> {
    fn default() -> Self {
        Self {
            select_all: None,
            sort_click: None,
            toggles: Vec::new(),
            actions: Vec::new(),
        }
    }
}

/// 「显示列」下拉菜单（两个 Tab 共用）：勾选/取消列，至少保留一列
pub(super) fn column_menu<C: PartialEq + Copy>(
    ui: &mut egui::Ui,
    all: &[C],
    selected: &mut Vec<C>,
    label: impl Fn(C) -> &'static str,
) {
    ui.set_min_width(150.0);
    ui.label(RichText::new("显示列").weak());
    ui.separator();
    for &col in all {
        let mut on = selected.contains(&col);
        if ui.checkbox(&mut on, label(col)).changed() {
            if on {
                // 按 all 的顺序重新并入，保证列顺序稳定
                let cur = selected.clone();
                *selected = all
                    .iter()
                    .copied()
                    .filter(|c| *c == col || cur.contains(c))
                    .collect();
            } else if selected.len() > 1 {
                selected.retain(|c| *c != col);
            }
        }
    }
    ui.separator();
    if ui.button("恢复默认列").clicked() {
        *selected = all.to_vec();
        ui.close();
    }
}

/// 状态列排序权重（运行中的排最前，其次创建/暂停，最后退出）
pub(super) fn state_rank(state: &str) -> u8 {
    match state {
        "running" => 0,
        "restarting" => 1,
        "paused" => 2,
        "created" => 3,
        "removing" => 4,
        "exited" => 5,
        "dead" => 6,
        _ => 7,
    }
}

/// 按指定列比较两个容器
pub(super) fn cmp_container(a: &ContainerInfo, b: &ContainerInfo, col: CtlCol) -> Ordering {
    match col {
        CtlCol::Id => a.id.cmp(&b.id),
        CtlCol::Name => a.name.cmp(&b.name),
        CtlCol::Image => a.image.cmp(&b.image),
        CtlCol::State => state_rank(&a.state).cmp(&state_rank(&b.state)),
        // 创建越晚 = 运行时长越短，升序即"时长从短到长"
        CtlCol::Uptime => b.created.cmp(&a.created),
        CtlCol::Ports => a.ports.cmp(&b.ports),
        CtlCol::Cpu => a.cpu_percent.total_cmp(&b.cpu_percent),
        CtlCol::Mem => a.mem_percent().total_cmp(&b.mem_percent()),
        CtlCol::Disk => a.size_rw.cmp(&b.size_rw),
        CtlCol::Net => (a.net_rx + a.net_tx).cmp(&(b.net_rx + b.net_tx)),
        CtlCol::Actions => Ordering::Equal,
    }
}

/// 按指定列比较两个镜像
pub(super) fn cmp_image(a: &ImageInfo, b: &ImageInfo, col: ImgCol) -> Ordering {
    match col {
        ImgCol::Name => a.name.cmp(&b.name),
        ImgCol::Tag => a.tag.cmp(&b.tag),
        ImgCol::Size => a.size.cmp(&b.size),
        ImgCol::Created => a.created.cmp(&b.created),
        ImgCol::Refs => b.containers.cmp(&a.containers),
        ImgCol::Actions => Ordering::Equal,
    }
}
