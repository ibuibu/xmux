#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone)]
pub enum LayoutNode {
    Leaf {
        pane_id: u32,
    },
    Split {
        direction: Split,
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

/// 画面上の矩形領域
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl LayoutNode {
    pub fn single(pane_id: u32) -> Self {
        LayoutNode::Leaf { pane_id }
    }

    /// レイアウトツリーから各ペインの矩形領域を計算
    pub fn compute_rects(&self, area: Rect) -> Vec<(u32, Rect)> {
        let mut result = Vec::new();
        self.compute_rects_inner(area, &mut result);
        result
    }

    fn compute_rects_inner(&self, area: Rect, result: &mut Vec<(u32, Rect)>) {
        match self {
            LayoutNode::Leaf { pane_id } => {
                result.push((*pane_id, area));
            }
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let (first_area, second_area) = split_rect(area, *direction, *ratio);
                first.compute_rects_inner(first_area, result);
                second.compute_rects_inner(second_area, result);
            }
        }
    }

    /// 指定ペインを含むLeafを分割して新しいペインを追加
    pub fn split_pane(&mut self, target_pane_id: u32, new_pane_id: u32, direction: Split) -> bool {
        match self {
            LayoutNode::Leaf { pane_id } => {
                if *pane_id == target_pane_id {
                    let old_id = *pane_id;
                    *self = LayoutNode::Split {
                        direction,
                        ratio: 0.5,
                        first: Box::new(LayoutNode::Leaf { pane_id: old_id }),
                        second: Box::new(LayoutNode::Leaf {
                            pane_id: new_pane_id,
                        }),
                    };
                    true
                } else {
                    false
                }
            }
            LayoutNode::Split { first, second, .. } => {
                first.split_pane(target_pane_id, new_pane_id, direction)
                    || second.split_pane(target_pane_id, new_pane_id, direction)
            }
        }
    }

    /// 指定ペインを削除し、兄弟ノードで置き換える。残ったペインIDを返す
    pub fn remove_pane(&mut self, target_pane_id: u32) -> Option<u32> {
        match self {
            LayoutNode::Leaf { .. } => None,
            LayoutNode::Split { first, second, .. } => {
                // firstが対象のLeafなら、secondで置き換え
                if matches!(first.as_ref(), LayoutNode::Leaf { pane_id } if *pane_id == target_pane_id)
                {
                    let sibling = *second.clone();
                    let remaining = sibling.first_pane_id();
                    *self = sibling;
                    return Some(remaining);
                }
                // secondが対象のLeafなら、firstで置き換え
                if matches!(second.as_ref(), LayoutNode::Leaf { pane_id } if *pane_id == target_pane_id)
                {
                    let sibling = *first.clone();
                    let remaining = sibling.first_pane_id();
                    *self = sibling;
                    return Some(remaining);
                }
                // 再帰的に探索
                first
                    .remove_pane(target_pane_id)
                    .or_else(|| second.remove_pane(target_pane_id))
            }
        }
    }

    /// ツリー内の最初のペインIDを返す
    pub fn first_pane_id(&self) -> u32 {
        match self {
            LayoutNode::Leaf { pane_id } => *pane_id,
            LayoutNode::Split { first, .. } => first.first_pane_id(),
        }
    }

    /// ペインID一覧を返す（テスト用）
    #[cfg(test)]
    pub fn pane_ids(&self) -> Vec<u32> {
        let mut ids = Vec::new();
        self.collect_pane_ids(&mut ids);
        ids
    }

    #[cfg(test)]
    fn collect_pane_ids(&self, ids: &mut Vec<u32>) {
        match self {
            LayoutNode::Leaf { pane_id } => ids.push(*pane_id),
            LayoutNode::Split { first, second, .. } => {
                first.collect_pane_ids(ids);
                second.collect_pane_ids(ids);
            }
        }
    }
}

/// ペイン間のボーダー線
#[derive(Debug, Clone, Copy)]
pub struct Border {
    pub x: u16,
    pub y: u16,
    pub length: u16,
    pub orientation: Split, // Vertical = │, Horizontal = ─
}

impl LayoutNode {
    /// レイアウトツリーからボーダー位置を計算
    pub fn compute_borders(&self, area: Rect) -> Vec<Border> {
        let mut borders = Vec::new();
        self.compute_borders_inner(area, &mut borders);
        borders
    }

    fn compute_borders_inner(&self, area: Rect, borders: &mut Vec<Border>) {
        if let LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } = self
        {
            let (first_area, second_area) = split_rect(area, *direction, *ratio);
            match direction {
                Split::Vertical => {
                    borders.push(Border {
                        x: first_area.x + first_area.width,
                        y: area.y,
                        length: area.height,
                        orientation: Split::Vertical,
                    });
                }
                Split::Horizontal => {
                    borders.push(Border {
                        x: area.x,
                        y: first_area.y + first_area.height,
                        length: area.width,
                        orientation: Split::Horizontal,
                    });
                }
            }
            first.compute_borders_inner(first_area, borders);
            second.compute_borders_inner(second_area, borders);
        }
    }
}

pub fn split_rect(area: Rect, direction: Split, ratio: f32) -> (Rect, Rect) {
    match direction {
        Split::Vertical => {
            // 左右に分割
            let first_width = ((area.width as f32) * ratio) as u16;
            let second_width = area.width.saturating_sub(first_width + 1); // 1列をボーダーに
            let first = Rect {
                x: area.x,
                y: area.y,
                width: first_width,
                height: area.height,
            };
            let second = Rect {
                x: area.x + first_width + 1,
                y: area.y,
                width: second_width,
                height: area.height,
            };
            (first, second)
        }
        Split::Horizontal => {
            // 上下に分割
            let first_height = ((area.height as f32) * ratio) as u16;
            let second_height = area.height.saturating_sub(first_height + 1); // 1行をボーダーに
            let first = Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: first_height,
            };
            let second = Rect {
                x: area.x,
                y: area.y + first_height + 1,
                width: area.width,
                height: second_height,
            };
            (first, second)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(x: u16, y: u16, w: u16, h: u16) -> Rect {
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn single_leaf_computes_full_area() {
        let layout = LayoutNode::single(0);
        let rects = layout.compute_rects(area(0, 0, 100, 50));
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].0, 0);
        assert_eq!(rects[0].1.width, 100);
        assert_eq!(rects[0].1.height, 50);
    }

    #[test]
    fn pane_ids_single() {
        let layout = LayoutNode::single(42);
        assert_eq!(layout.pane_ids(), vec![42]);
    }

    #[test]
    fn first_pane_id_single() {
        let layout = LayoutNode::single(7);
        assert_eq!(layout.first_pane_id(), 7);
    }

    #[test]
    fn split_pane_vertical() {
        let mut layout = LayoutNode::single(0);
        assert!(layout.split_pane(0, 1, Split::Vertical));
        assert_eq!(layout.pane_ids(), vec![0, 1]);
    }

    #[test]
    fn split_pane_nonexistent_target() {
        let mut layout = LayoutNode::single(0);
        assert!(!layout.split_pane(99, 1, Split::Vertical));
        assert_eq!(layout.pane_ids(), vec![0]);
    }

    #[test]
    fn split_vertical_rects() {
        let mut layout = LayoutNode::single(0);
        layout.split_pane(0, 1, Split::Vertical);
        let rects = layout.compute_rects(area(0, 0, 101, 50));
        assert_eq!(rects.len(), 2);
        let (id0, r0) = &rects[0];
        let (id1, r1) = &rects[1];
        assert_eq!(*id0, 0);
        assert_eq!(*id1, 1);
        assert_eq!(r0.x, 0);
        assert_eq!(r0.width, 50);
        assert_eq!(r1.x, 51);
        assert_eq!(r1.width, 50);
        assert_eq!(r0.height, 50);
        assert_eq!(r1.height, 50);
    }

    #[test]
    fn split_horizontal_rects() {
        let mut layout = LayoutNode::single(0);
        layout.split_pane(0, 1, Split::Horizontal);
        let rects = layout.compute_rects(area(0, 0, 80, 41));
        assert_eq!(rects.len(), 2);
        let (_, r0) = &rects[0];
        let (_, r1) = &rects[1];
        assert_eq!(r0.y, 0);
        assert_eq!(r0.height, 20);
        assert_eq!(r1.y, 21);
        assert_eq!(r1.height, 20);
        assert_eq!(r0.width, 80);
        assert_eq!(r1.width, 80);
    }

    #[test]
    fn remove_pane_from_split() {
        let mut layout = LayoutNode::single(0);
        layout.split_pane(0, 1, Split::Vertical);
        let remaining = layout.remove_pane(0);
        assert_eq!(remaining, Some(1));
        assert_eq!(layout.pane_ids(), vec![1]);
    }

    #[test]
    fn remove_second_pane_from_split() {
        let mut layout = LayoutNode::single(0);
        layout.split_pane(0, 1, Split::Vertical);
        let remaining = layout.remove_pane(1);
        assert_eq!(remaining, Some(0));
        assert_eq!(layout.pane_ids(), vec![0]);
    }

    #[test]
    fn remove_pane_from_leaf_returns_none() {
        let mut layout = LayoutNode::single(0);
        assert_eq!(layout.remove_pane(0), None);
    }

    #[test]
    fn remove_nonexistent_pane() {
        let mut layout = LayoutNode::single(0);
        layout.split_pane(0, 1, Split::Vertical);
        assert_eq!(layout.remove_pane(99), None);
    }

    #[test]
    fn nested_split_and_remove() {
        let mut layout = LayoutNode::single(0);
        layout.split_pane(0, 1, Split::Vertical);
        layout.split_pane(1, 2, Split::Horizontal);
        assert_eq!(layout.pane_ids(), vec![0, 1, 2]);

        let remaining = layout.remove_pane(1);
        assert_eq!(remaining, Some(2));
        assert_eq!(layout.pane_ids(), vec![0, 2]);
    }

    #[test]
    fn split_rect_vertical() {
        let (first, second) = split_rect(area(10, 0, 81, 40), Split::Vertical, 0.5);
        assert_eq!(first.x, 10);
        assert_eq!(first.width, 40);
        assert_eq!(second.x, 51);
        assert_eq!(second.width, 40);
    }

    #[test]
    fn split_rect_horizontal() {
        let (first, second) = split_rect(area(0, 5, 80, 41), Split::Horizontal, 0.5);
        assert_eq!(first.y, 5);
        assert_eq!(first.height, 20);
        assert_eq!(second.y, 26);
        assert_eq!(second.height, 20);
    }

    #[test]
    fn compute_rects_with_offset() {
        let layout = LayoutNode::single(0);
        let rects = layout.compute_rects(area(22, 0, 58, 40));
        assert_eq!(rects[0].1.x, 22);
        assert_eq!(rects[0].1.width, 58);
    }

    #[test]
    fn first_pane_id_nested() {
        let mut layout = LayoutNode::single(5);
        layout.split_pane(5, 10, Split::Vertical);
        layout.split_pane(10, 15, Split::Horizontal);
        assert_eq!(layout.first_pane_id(), 5);
    }

    #[test]
    fn no_borders_for_single_pane() {
        let layout = LayoutNode::single(0);
        let borders = layout.compute_borders(area(0, 0, 80, 40));
        assert!(borders.is_empty());
    }

    #[test]
    fn vertical_split_has_one_vertical_border() {
        let mut layout = LayoutNode::single(0);
        layout.split_pane(0, 1, Split::Vertical);
        let borders = layout.compute_borders(area(0, 0, 101, 50));
        assert_eq!(borders.len(), 1);
        assert_eq!(borders[0].orientation, Split::Vertical);
        assert_eq!(borders[0].x, 50); // first half is 50 cols wide
        assert_eq!(borders[0].y, 0);
        assert_eq!(borders[0].length, 50);
    }

    #[test]
    fn horizontal_split_has_one_horizontal_border() {
        let mut layout = LayoutNode::single(0);
        layout.split_pane(0, 1, Split::Horizontal);
        let borders = layout.compute_borders(area(0, 0, 80, 41));
        assert_eq!(borders.len(), 1);
        assert_eq!(borders[0].orientation, Split::Horizontal);
        assert_eq!(borders[0].y, 20);
        assert_eq!(borders[0].x, 0);
        assert_eq!(borders[0].length, 80);
    }

    #[test]
    fn nested_split_has_two_borders() {
        let mut layout = LayoutNode::single(0);
        layout.split_pane(0, 1, Split::Vertical);
        layout.split_pane(1, 2, Split::Horizontal);
        let borders = layout.compute_borders(area(0, 0, 101, 50));
        assert_eq!(borders.len(), 2);
    }
}
