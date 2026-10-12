//! 容器行预处理：状态/关键词过滤 + 表头排序 + compose 分组展示序列

use std::collections::BTreeMap;

use super::*;

impl DvApp {
    /// 过滤排序后的容器行、表格展示序列（含 compose 组头）与小写搜索词
    pub(super) fn build_disp(&self) -> (Vec<ContainerInfo>, Vec<DispRow>, String) {
        // 过滤：状态筛选 + 搜索关键词（名称/ID/镜像/端口/状态，不区分大小写）
        let q = self.container_query.trim().to_lowercase();
        let mut rows: Vec<ContainerInfo> = self
            .containers
            .iter()
            .filter(|c| match self.filter {
                Filter::Running => c.is_running(),
                Filter::All => true,
            })
            .filter(|c| {
                if q.is_empty() {
                    return true;
                }
                c.name.to_lowercase().contains(&q)
                    || c.id.to_lowercase().contains(&q)
                    || c.image.to_lowercase().contains(&q)
                    || c.ports.to_lowercase().contains(&q)
                    || c.state.to_lowercase().contains(&q)
            })
            .cloned()
            .collect();

        // 排序：点表头切换；恒以 ID 兜底，保证刷新前后行序稳定不跳
        let (sort_col, ascending) = self.ctl_sort;
        rows.sort_by(|a, b| {
            let ord = cmp_container(a, b, sort_col);
            let ord = if ascending { ord } else { ord.reverse() };
            ord.then_with(|| a.id.cmp(&b.id))
        });

        // 展示序列：开启分组时插入 compose 组头，折叠的组不展开成员
        let disp: Vec<DispRow> = if self.group_compose {
            let mut plain: Vec<usize> = Vec::new();
            let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
            for (i, c) in rows.iter().enumerate() {
                match &c.compose_project {
                    Some(p) => groups.entry(p.clone()).or_default().push(i),
                    None => plain.push(i),
                }
            }
            let mut out: Vec<DispRow> = plain.into_iter().map(DispRow::Container).collect();
            for (project, members) in groups {
                let collapsed = self.collapsed_groups.contains(&project);
                let count = members.len();
                out.push(DispRow::Group { project, count });
                if !collapsed {
                    out.extend(members.into_iter().map(DispRow::Container));
                }
            }
            out
        } else {
            (0..rows.len()).map(DispRow::Container).collect()
        };

        (rows, disp, q)
    }
}
