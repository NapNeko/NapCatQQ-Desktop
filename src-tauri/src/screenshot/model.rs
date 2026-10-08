// 截图选区和有界标注历史，坐标使用桌面物理像素。

pub const MAX_ANNOTATIONS: usize = 256;
pub const MAX_POINTS: usize = 4096;
pub const MAX_TOTAL_POINTS: usize = 65_536;
pub const MAX_TEXT_CHARS: usize = 2048;
pub const MAX_TEXT_LINES: usize = 64;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.x.abs() <= 1_000_000.0
            && self.y.abs() <= 1_000_000.0
    }

    pub fn distance_squared(self, other: Self) -> f32 {
        (self.x - other.x).powi(2) + (self.y - other.y).powi(2)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PixelRect {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    pub fn right(self) -> i32 {
        (i64::from(self.left) + i64::from(self.width)).clamp(i32::MIN as i64, i32::MAX as i64)
            as i32
    }

    pub fn bottom(self) -> i32 {
        (i64::from(self.top) + i64::from(self.height)).clamp(i32::MIN as i64, i32::MAX as i64)
            as i32
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.left as f32
            && point.y >= self.top as f32
            && point.x < self.right() as f32
            && point.y < self.bottom() as f32
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let left = self.left.max(other.left);
        let top = self.top.max(other.top);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        (right > left && bottom > top).then(|| Self {
            left,
            top,
            width: (i64::from(right) - i64::from(left)) as u32,
            height: (i64::from(bottom) - i64::from(top)) as u32,
        })
    }

    pub fn from_points(a: Point, b: Point) -> Self {
        if !a.valid() || !b.valid() {
            return Self::default();
        }
        let left = a.x.min(b.x).floor() as i32;
        let top = a.y.min(b.y).floor() as i32;
        let right = a.x.max(b.x).ceil() as i32;
        let bottom = a.y.max(b.y).ceil() as i32;
        Self {
            left,
            top,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        }
    }

    pub fn origin(self) -> Point {
        Point {
            x: self.left as f32,
            y: self.top as f32,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Select,
    Rectangle,
    Ellipse,
    Arrow,
    Pen,
    Text,
    Number,
    Mosaic,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub tool: Tool,
    pub points: Vec<Point>,
    pub color: [u8; 4],
    pub width: f32,
    pub text: String,
    pub number: u32,
}

impl Annotation {
    pub fn new(tool: Tool, point: Point, color: [u8; 4], width: f32) -> Self {
        Self {
            tool,
            points: vec![point],
            color,
            width,
            text: String::new(),
            number: 0,
        }
    }

    pub fn push_point(&mut self, point: Point) -> bool {
        if !point.valid()
            || self.points.len() >= MAX_POINTS
            || self
                .points
                .last()
                .is_some_and(|last| last.distance_squared(point) < 1.0)
        {
            return false;
        }
        self.points.push(point);
        true
    }

    fn validate(&self) -> Result<(), String> {
        if self.tool == Tool::Select {
            return Err("选区不能作为标注保存".into());
        }
        let minimum = match self.tool {
            Tool::Text | Tool::Number | Tool::Pen => 1,
            _ => 2,
        };
        if self.points.len() < minimum
            || self.points.len() > MAX_POINTS
            || self.points.iter().any(|point| !point.valid())
        {
            return Err("标注坐标无效或笔画过长".into());
        }
        if !self.width.is_finite() || !(0.5..=64.0).contains(&self.width) {
            return Err("标注粗细无效".into());
        }
        if self.text.chars().count() > MAX_TEXT_CHARS {
            return Err("标注文字最多 2048 字".into());
        }
        if self.text.split('\n').count() > MAX_TEXT_LINES {
            return Err("标注文字最多 64 行".into());
        }
        if self.tool == Tool::Text && self.text.trim().is_empty() {
            return Err("请输入标注文字".into());
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Editor {
    pub selection: Option<PixelRect>,
    pub annotations: Vec<Annotation>,
    pub tool: Tool,
    pub color: [u8; 4],
    pub stroke: f32,
    redo: Vec<Annotation>,
    revision: u64,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    pub fn new() -> Self {
        Self {
            selection: None,
            annotations: Vec::new(),
            tool: Tool::Select,
            color: [242, 79, 92, 255],
            stroke: 3.0,
            redo: Vec::new(),
            revision: 1,
        }
    }

    pub fn commit(&mut self, mut annotation: Annotation) -> Result<(), String> {
        annotation.validate()?;
        if self.annotations.len() >= MAX_ANNOTATIONS {
            return Err("一张截图最多 256 个标注".into());
        }
        let points: usize = self.annotations.iter().map(|item| item.points.len()).sum();
        if points + annotation.points.len() > MAX_TOTAL_POINTS {
            return Err("截图笔画已达到上限".into());
        }
        if annotation.tool == Tool::Number && annotation.number == 0 {
            annotation.number = self.next_number();
        }
        self.redo.clear();
        self.annotations.push(annotation);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        let Some(annotation) = self.annotations.pop() else {
            return false;
        };
        // 所有权在两条栈之间移动，撤销不复制图像或笔画。
        self.redo.push(annotation);
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(annotation) = self.redo.pop() else {
            return false;
        };
        self.annotations.push(annotation);
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.annotations.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn next_number(&self) -> u32 {
        self.annotations
            .iter()
            .filter(|annotation| annotation.tool == Tool::Number)
            .map(|annotation| annotation.number)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(tool: Tool) -> Annotation {
        Annotation::new(tool, Point { x: -40.0, y: 25.0 }, [240, 60, 80, 255], 3.0)
    }

    #[test]
    fn reversed_drag_preserves_negative_monitor_coordinates() {
        let rect = PixelRect::from_points(Point { x: -10.2, y: 80.6 }, Point { x: -99.8, y: -4.2 });
        assert_eq!(
            rect,
            PixelRect {
                left: -100,
                top: -5,
                width: 90,
                height: 86
            }
        );
        assert!(rect.contains(Point { x: -50.0, y: 0.0 }));
        assert!(!rect.contains(Point { x: -10.0, y: 0.0 }));
        assert!(!rect.contains(Point { x: -50.0, y: 81.0 }));
    }

    #[test]
    fn intersection_is_symmetric_and_excludes_touching_edges() {
        let a = PixelRect {
            left: -100,
            top: -50,
            width: 150,
            height: 100,
        };
        let b = PixelRect {
            left: -20,
            top: 0,
            width: 100,
            height: 70,
        };
        let expected = Some(PixelRect {
            left: -20,
            top: 0,
            width: 70,
            height: 50,
        });
        assert_eq!(a.intersection(b), expected);
        assert_eq!(b.intersection(a), expected);
        assert_eq!(
            a.intersection(PixelRect {
                left: 50,
                top: 0,
                width: 5,
                height: 5
            }),
            None
        );
    }

    #[test]
    fn invalid_drag_and_extreme_rect_edges_do_not_overflow() {
        assert_eq!(
            PixelRect::from_points(
                Point {
                    x: f32::NAN,
                    y: 0.0
                },
                Point::default()
            ),
            PixelRect::default()
        );
        let rect = PixelRect {
            left: i32::MAX - 2,
            top: i32::MAX - 2,
            width: u32::MAX,
            height: u32::MAX,
        };
        assert_eq!(rect.right(), i32::MAX);
        assert_eq!(rect.bottom(), i32::MAX);
    }

    #[test]
    fn undo_and_redo_move_owned_strokes_and_keep_number_sequence() {
        let mut editor = Editor::new();
        editor.commit(mark(Tool::Number)).unwrap();
        editor.commit(mark(Tool::Number)).unwrap();
        let points = editor.annotations[1].points.as_ptr();
        assert_eq!(editor.next_number(), 3);
        assert!(editor.undo());
        assert_eq!(editor.next_number(), 2);
        assert!(editor.can_redo());
        assert!(editor.redo());
        assert_eq!(editor.annotations[1].points.as_ptr(), points);
        assert_eq!(editor.annotations[1].number, 2);
        assert!(!editor.can_redo());
    }

    #[test]
    fn new_commit_invalidates_redo_but_failed_commit_preserves_history() {
        let mut editor = Editor::new();
        editor.commit(mark(Tool::Number)).unwrap();
        assert!(editor.undo());
        let revision = editor.revision();
        assert!(editor.commit(mark(Tool::Text)).is_err());
        assert_eq!(editor.revision(), revision);
        assert!(editor.can_redo());
        editor.commit(mark(Tool::Pen)).unwrap();
        assert!(!editor.can_redo());
        assert!(!editor.redo());
    }

    #[test]
    fn annotation_and_total_point_budgets_reject_without_losing_existing_work() {
        let mut editor = Editor::new();
        for _ in 0..MAX_ANNOTATIONS {
            editor.commit(mark(Tool::Number)).unwrap();
        }
        assert!(editor.commit(mark(Tool::Number)).is_err());
        assert_eq!(editor.annotations.len(), MAX_ANNOTATIONS);
        let mut editor = Editor::new();
        for _ in 0..MAX_TOTAL_POINTS / MAX_POINTS {
            let mut pen = mark(Tool::Pen);
            pen.points = vec![Point::default(); MAX_POINTS];
            editor.commit(pen).unwrap();
        }
        assert!(editor.commit(mark(Tool::Pen)).is_err());
        assert_eq!(
            editor
                .annotations
                .iter()
                .map(|item| item.points.len())
                .sum::<usize>(),
            MAX_TOTAL_POINTS
        );
    }

    #[test]
    fn active_pen_filters_tiny_moves_and_stops_at_point_limit() {
        let mut pen = mark(Tool::Pen);
        assert!(!pen.push_point(Point { x: -39.5, y: 25.0 }));
        assert!(!pen.push_point(Point {
            x: f32::INFINITY,
            y: 25.0
        }));
        pen.points = vec![Point::default(); MAX_POINTS];
        assert!(!pen.push_point(Point { x: 5.0, y: 5.0 }));
        pen.points.push(Point::default());
        assert!(Editor::new().commit(pen).is_err());
    }

    #[test]
    fn text_budget_counts_unicode_characters_and_limits_layout_rows() {
        let mut text = mark(Tool::Text);
        text.text = "截图".repeat(MAX_TEXT_CHARS / 2);
        assert!(Editor::new().commit(text.clone()).is_ok());
        text.text.push('字');
        assert!(Editor::new().commit(text).is_err());
        let mut text = mark(Tool::Text);
        text.text = "行\n".repeat(MAX_TEXT_LINES);
        assert!(Editor::new().commit(text).is_err());
    }

    #[test]
    fn malformed_annotation_is_rejected_before_history_changes() {
        let mut pen = mark(Tool::Pen);
        pen.width = f32::NAN;
        let mut editor = Editor::new();
        assert!(editor.commit(pen).is_err());
        let mut rectangle = mark(Tool::Rectangle);
        rectangle.points.push(Point {
            x: f32::INFINITY,
            y: 0.0,
        });
        assert!(editor.commit(rectangle).is_err());
        assert!(!editor.can_undo());
    }
}
