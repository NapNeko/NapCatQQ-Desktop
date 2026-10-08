// 滚动截图按可靠重叠向两端扩展，保留可撤销的有界条带。
use std::collections::VecDeque;

const MAX_BYTES: usize = 128 * 1024 * 1024;
const MAX_HEIGHT: u32 = 24_000;
const MIN_STRIP_ROWS: usize = 64;

pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}
impl Frame {
    pub fn validate(&self) -> Result<(), String> {
        let bytes = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|n| n.checked_mul(4));
        if self.width < 32
            || self.height < 96
            || bytes != Some(self.bgra.len())
            || self.bgra.len() > 32 * 1024 * 1024
        {
            return Err("滚动选区过小或超过 32 MiB 像素限制".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Append {
    Added(u32),
    Unchanged,
    Covered,
    NoOverlap,
    Ambiguous,
    Limit,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Edge {
    Top,
    Bottom,
}

pub struct Stitcher {
    width: u32,
    viewport: u32,
    previous: Vec<u8>,
    strips: VecDeque<Vec<u8>>,
    history: Vec<Edge>,
    header: Vec<u8>,
    footer: Vec<u8>,
    top: u32,
    bottom: u32,
    edges_locked: bool,
    rows: u32,
    observed_offset: i64,
    min_offset: i64,
    max_offset: i64,
}
impl Stitcher {
    pub fn new(frame: Frame) -> Result<Self, String> {
        frame.validate()?;
        Ok(Self {
            width: frame.width,
            viewport: frame.height,
            previous: frame.bgra.clone(),
            strips: VecDeque::from([frame.bgra]),
            history: Vec::new(),
            header: Vec::new(),
            footer: Vec::new(),
            top: 0,
            bottom: 0,
            edges_locked: false,
            rows: frame.height,
            observed_offset: 0,
            min_offset: 0,
            max_offset: 0,
        })
    }
    pub fn height(&self) -> u32 {
        self.rows
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn parts(&self) -> usize {
        self.strips.len()
    }
    pub fn can_undo(&self) -> bool {
        !self.history.is_empty()
    }
    pub fn append(&mut self, frame: Frame) -> Result<Append, String> {
        frame.validate()?;
        if frame.width != self.width || frame.height != self.viewport {
            return Err("滚动选区的尺寸已变化".into());
        }
        // 整图平均会淹没白底上的少量文字，1px 滚动也必须交给匹配器判断。
        if self.previous == frame.bgra {
            return Ok(Append::Unchanged);
        }
        let (shift, ambiguous) = find_shift(
            &self.previous,
            &frame.bgra,
            self.width,
            self.viewport,
            self.top,
            self.bottom,
        );
        let Some(shift) = shift else {
            return Ok(Append::NoOverlap);
        };
        if ambiguous {
            return Ok(Append::Ambiguous);
        }
        if shift == 0 {
            return Ok(Append::Unchanged);
        }
        let first_append = !self.edges_locked;
        let (top, bottom) = if first_append {
            fixed_edges(
                &self.previous,
                &frame.bgra,
                self.width,
                self.viewport,
                shift,
            )
        } else {
            (self.top, self.bottom)
        };
        let next_offset = self.observed_offset + i64::from(shift);
        let (edge, added) = if next_offset < self.min_offset {
            (Edge::Top, (self.min_offset - next_offset) as u32)
        } else if next_offset > self.max_offset {
            (Edge::Bottom, (next_offset - self.max_offset) as u32)
        } else {
            self.previous = frame.bgra;
            self.observed_offset = next_offset;
            return Ok(Append::Covered);
        };
        if added > self.viewport - top - bottom {
            return Ok(Append::NoOverlap);
        }
        let bytes = (self.rows.saturating_add(added) as usize)
            .checked_mul(self.width as usize)
            .and_then(|n| n.checked_mul(4));
        if self.rows.saturating_add(added) > MAX_HEIGHT || bytes.is_none_or(|n| n > MAX_BYTES) {
            return Ok(Append::Limit);
        }
        let stride = self.width as usize * 4;
        if first_append {
            self.edges_locked = true;
            self.top = top;
            self.bottom = bottom;
            self.header = self.previous[..top as usize * stride].to_vec();
            self.footer = self.previous[(self.viewport - bottom) as usize * stride..].to_vec();
            if let Some(first) = self.strips.front_mut() {
                first.truncate((self.viewport - bottom) as usize * stride);
                drop(first.drain(..top as usize * stride));
            }
        }
        let body_end = (self.viewport - self.bottom) as usize * stride;
        let start = match edge {
            Edge::Top => self.top as usize * stride,
            Edge::Bottom => body_end - added as usize * stride,
        };
        let new_rows = &frame.bgra[start..start + added as usize * stride];
        // 平滑滚动常常每帧只多几行；按小段合并，让容量与长度相关而不是与帧数相关。
        let outer = match edge {
            Edge::Top => self.strips.front_mut(),
            Edge::Bottom => self.strips.back_mut(),
        };
        if self.history.last() == Some(&edge)
            && let Some(strip) = outer.filter(|strip| strip.len() / stride < MIN_STRIP_ROWS)
        {
            match edge {
                Edge::Top => {
                    let old_len = strip.len();
                    strip.resize(old_len + new_rows.len(), 0);
                    strip.copy_within(..old_len, new_rows.len());
                    strip[..new_rows.len()].copy_from_slice(new_rows);
                }
                Edge::Bottom => strip.extend_from_slice(new_rows),
            }
        } else {
            match edge {
                Edge::Top => self.strips.push_front(new_rows.to_vec()),
                Edge::Bottom => self.strips.push_back(new_rows.to_vec()),
            }
            self.history.push(edge);
        }
        match edge {
            Edge::Top => {
                self.header
                    .copy_from_slice(&frame.bgra[..self.top as usize * stride]);
                self.min_offset = next_offset;
            }
            Edge::Bottom => {
                self.footer.copy_from_slice(&frame.bgra[body_end..]);
                self.max_offset = next_offset;
            }
        }
        self.previous = frame.bgra;
        self.observed_offset = next_offset;
        self.rows += added;
        Ok(Append::Added(added))
    }
    pub fn undo(&mut self) -> bool {
        let Some(edge) = self.history.pop() else {
            return false;
        };
        let strip = match edge {
            Edge::Top => self.strips.pop_front(),
            Edge::Bottom => self.strips.pop_back(),
        }
        .expect("每项采集历史对应一段新增内容");
        let stride = self.width as usize * 4;
        let removed = (strip.len() / stride) as u32;
        self.rows -= removed;
        self.observed_offset = match edge {
            Edge::Top => {
                self.min_offset += i64::from(removed);
                self.min_offset
            }
            Edge::Bottom => {
                self.max_offset -= i64::from(removed);
                self.max_offset
            }
        };
        let content_bytes = (self.viewport - self.top - self.bottom) as usize * stride;
        let mut skip = match edge {
            Edge::Top => 0,
            Edge::Bottom => (self.rows - self.viewport) as usize * stride,
        };
        let mut remaining = content_bytes;
        self.previous.clear();
        self.previous.extend_from_slice(&self.header);
        for strip in &self.strips {
            let start = skip.min(strip.len());
            skip -= start;
            let count = remaining.min(strip.len() - start);
            self.previous
                .extend_from_slice(&strip[start..start + count]);
            remaining -= count;
            if remaining == 0 {
                break;
            }
        }
        self.previous.extend_from_slice(&self.footer);
        true
    }
    pub fn finish(self) -> Frame {
        let mut bgra = Vec::with_capacity(self.width as usize * self.rows as usize * 4);
        bgra.extend_from_slice(&self.header);
        for strip in self.strips {
            bgra.extend_from_slice(&strip);
        }
        bgra.extend_from_slice(&self.footer);
        Frame {
            width: self.width,
            height: self.rows,
            bgra,
        }
    }
    pub fn thumbnail(&self) -> Frame {
        self.thumbnail_region(0, self.rows, 96, 480)
    }
    pub fn detail_preview(&self, width: u32, height: u32) -> (Frame, (u32, u32)) {
        let width = width.max(1);
        let height = height.max(1);
        let rows = ((u64::from(self.width) * u64::from(height) / u64::from(width))
            .max(1)
            .min(u64::from(self.rows))) as u32;
        let travel = self.max_offset - self.min_offset;
        let start = if travel > 0 {
            ((self.observed_offset - self.min_offset).clamp(0, travel)
                * i64::from(self.rows - rows)
                / travel) as u32
        } else {
            0
        };
        (
            self.thumbnail_region(start, rows, width, height),
            (start, start + rows),
        )
    }
    fn thumbnail_region(&self, start: u32, rows: u32, max_width: u32, max_height: u32) -> Frame {
        let scale = (max_width as f64 / self.width as f64)
            .min(max_height as f64 / rows as f64)
            .min(1.0);
        let width = (self.width as f64 * scale).round().max(1.0) as u32;
        let height = (rows as f64 * scale).round().max(1.0) as u32;
        let mut bgra = Vec::with_capacity(width as usize * height as usize * 4);
        let stride = self.width as usize * 4;
        let mut strips = std::iter::once(&self.header)
            .chain(self.strips.iter())
            .chain(std::iter::once(&self.footer));
        let mut strip = strips.next().expect("预览包含页头和正文");
        let mut strip_start = 0;
        for y in 0..height {
            let source_y = start as usize + y as usize * rows as usize / height as usize;
            let source = source_y * stride;
            while source >= strip_start + strip.len() {
                strip_start += strip.len();
                strip = strips.next().expect("预览范围位于已拼接内容内");
            }
            let row = source - strip_start;
            let selected = &strip[row..row + stride];
            for x in 0..width {
                let at = (x as usize * self.width as usize / width as usize) * 4;
                bgra.extend_from_slice(&selected[at..at + 4]);
            }
        }
        Frame {
            width,
            height,
            bgra,
        }
    }
}

fn fixed_edges(a: &[u8], b: &[u8], width: u32, height: u32, shift: i32) -> (u32, u32) {
    let stride = width as usize * 4;
    let (left, right) = moving_columns(a, b, width as usize, height as usize, shift);
    let same = |y: u32| {
        let row = y as usize * stride;
        let mut changed = 0;
        for x in (left..right).step_by(((right - left) / 384).max(1)) {
            if (0..3).any(|c| a[row + x * 4 + c].abs_diff(b[row + x * 4 + c]) > 8) {
                changed += 1;
                if changed >= 2 {
                    return false;
                }
            }
        }
        true
    };
    let limit = (height / 4).min(192);
    let mut top = 0;
    while top < limit && same(top) {
        top += 1;
    }
    let mut bottom = 0;
    while bottom < limit && same(height - bottom - 1) {
        bottom += 1;
    }
    // 纯空白边缘可能正在滚动，不能把偶然相同的空白吞成固定栏。
    let textured = |start: usize, end: usize| {
        let mut most_edges = 0;
        for row in (start..end).step_by(stride) {
            let mut edges = 0;
            let mut previous = None;
            for x in (left..right).step_by(((right - left) / 192).max(1)) {
                let at = row + x * 4;
                let rgb = [a[at], a[at + 1], a[at + 2]];
                if let Some(prior) = previous {
                    let prior: [u8; 3] = prior;
                    if rgb.iter().zip(prior).any(|(&c, p)| c.abs_diff(p) >= 16) {
                        edges += 1;
                    }
                }
                previous = Some(rgb);
            }
            most_edges = most_edges.max(edges);
        }
        most_edges >= 3
    };
    // 先有可靠位移，再检测固定区域；位移同样能解释的纹理属于正文。
    let moving_texture = |start: u32, end: u32| {
        let mut compared = 0;
        let mut matched = 0;
        for y in start..end {
            let shifted = i64::from(y) + i64::from(shift);
            if !(0..i64::from(height)).contains(&shifted) {
                continue;
            }
            let row = y as usize * stride;
            if !textured(row, row + stride) {
                continue;
            }
            let old_row = shifted as usize * stride;
            let mut error = 0u64;
            let mut samples = 0;
            for x in (left..right).step_by(((right - left) / 192).max(1)) {
                for c in 0..3 {
                    error += u64::from(a[old_row + x * 4 + c].abs_diff(b[row + x * 4 + c]));
                    samples += 1;
                }
            }
            compared += 1;
            matched += usize::from(error <= samples);
        }
        compared > 0 && matched * 4 >= compared * 3
    };
    if !textured(0, top as usize * stride) || moving_texture(0, top) {
        top = 0;
    }
    if !textured(
        (height - bottom) as usize * stride,
        height as usize * stride,
    ) || moving_texture(height - bottom, height)
    {
        bottom = 0;
    }
    // 页脚与正文常用同一底色；分隔线比连续相同的空白更能确定真实边界。
    let separator = |y: u32| {
        let mut changed = 0;
        let mut count = 0;
        for x in (left..right).step_by(((right - left) / 192).max(1)) {
            let at = (y as usize * width as usize + x) * 4;
            changed += usize::from((0..3).any(|c| a[at + c].abs_diff(a[at - stride + c]) > 8));
            count += 1;
        }
        count > 0 && changed * 4 >= count * 3
    };
    if let Some(boundary) = (1..=top).rev().find(|&y| separator(y)) {
        top = boundary;
    }
    if bottom > 0 {
        bottom = (height - bottom..height)
            .find(|&y| y > 0 && separator(y))
            .map_or(bottom, |boundary| height - boundary);
    }
    (top, bottom)
}

fn moving_columns(a: &[u8], b: &[u8], width: usize, height: usize, shift: i32) -> (usize, usize) {
    let mut left = width;
    let mut right = 0;
    let step = (width / 128).max(1);
    for x in (width / 24..width - width / 24).step_by(step) {
        let mut support = 0;
        for y in (0..height).step_by(2) {
            let old_y = y as i64 + i64::from(shift);
            if !(0..height as i64).contains(&old_y) {
                continue;
            }
            let at = (y * width + x) * 4;
            let old = (old_y as usize * width + x) * 4;
            let changed = (0..3).any(|c| a[at + c].abs_diff(b[at + c]) > 12);
            let aligned = (0..3).all(|c| a[old + c].abs_diff(b[at + c]) <= 4);
            if changed && aligned {
                support += 1;
            }
        }
        if support >= 2 {
            left = left.min(x.saturating_sub(step));
            right = right.max((x + step + 1).min(width));
        }
    }
    if left < right {
        (left, right)
    } else {
        (0, width)
    }
}

#[path = "alignment.rs"]
mod alignment;

fn find_shift(
    a: &[u8],
    b: &[u8],
    width: u32,
    height: u32,
    top: u32,
    bottom: u32,
) -> (Option<i32>, bool) {
    alignment::find_shift(a, b, width, height, top, bottom)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pixel(x: u32, y: u32) -> [u8; 4] {
        let mut n = x.wrapping_mul(0x9e3779b9) ^ y.wrapping_mul(0x85ebca6b);
        n ^= n >> 16;
        n = n.wrapping_mul(0x7feb352d);
        n ^= n >> 15;
        [n as u8, (n >> 8) as u8, (n >> 16) as u8, 255]
    }
    fn frame(offset: u32, height: u32, top: u32, bottom: u32) -> Frame {
        let width = 96;
        let mut bgra = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let line = if y < top {
                    1_000_000 + y
                } else if y >= height - bottom {
                    2_000_000 + y - (height - bottom)
                } else {
                    offset + y - top
                };
                bgra.extend_from_slice(&pixel(x, line));
            }
        }
        Frame {
            width,
            height,
            bgra,
        }
    }
    #[test]
    fn downward_stitch_preserves_every_row_and_keeps_fixed_bars_once() {
        let mut value = Stitcher::new(frame(0, 256, 32, 24)).unwrap();
        assert_eq!(
            value.append(frame(80, 256, 32, 24)).unwrap(),
            Append::Added(80)
        );
        assert_eq!(
            value.append(frame(140, 256, 32, 24)).unwrap(),
            Append::Added(60)
        );
        let result = value.finish();
        assert_eq!(result.height, 396);
        assert_eq!(result.bgra, frame(0, 396, 32, 24).bgra);
    }
    #[test]
    fn repeated_frames_and_backwards_scroll_do_not_duplicate_or_destroy_content() {
        let mut value = Stitcher::new(frame(0, 256, 0, 0)).unwrap();
        assert_eq!(
            value.append(frame(0, 256, 0, 0)).unwrap(),
            Append::Unchanged
        );
        assert_eq!(
            value.append(frame(80, 256, 0, 0)).unwrap(),
            Append::Added(80)
        );
        assert_eq!(value.append(frame(40, 256, 0, 0)).unwrap(), Append::Covered);
        assert_eq!(value.height(), 336);
        assert_eq!(value.parts(), 2);
    }
    #[test]
    fn undo_restores_match_anchor_and_allows_recollecting_the_last_strip() {
        let mut value = Stitcher::new(frame(0, 256, 32, 24)).unwrap();
        value.append(frame(80, 256, 32, 24)).unwrap();
        value.append(frame(140, 256, 32, 24)).unwrap();
        assert!(value.undo());
        assert_eq!(value.height(), 336);
        assert_eq!(value.previous, frame(80, 256, 32, 24).bgra);
        assert_eq!(
            value.append(frame(140, 256, 32, 24)).unwrap(),
            Append::Added(60)
        );
    }
    #[test]
    fn missing_overlap_and_ambiguous_blank_content_keep_the_last_good_image() {
        let mut value = Stitcher::new(frame(0, 256, 0, 0)).unwrap();
        assert_eq!(
            value.append(frame(900, 256, 0, 0)).unwrap(),
            Append::NoOverlap
        );
        assert_eq!(value.height(), 256);
        assert_eq!(value.parts(), 1);
        let mut blank = Frame {
            width: 96,
            height: 256,
            bgra: vec![255; 96 * 256 * 4],
        };
        let mut empty = Stitcher::new(Frame {
            width: 96,
            height: 256,
            bgra: blank.bgra.clone(),
        })
        .unwrap();
        for px in blank.bgra.chunks_exact_mut(4) {
            px[..3].fill(225);
        }
        assert!(matches!(
            empty.append(blank).unwrap(),
            Append::Unchanged | Append::NoOverlap | Append::Ambiguous
        ));
        assert_eq!(empty.height(), 256);
    }
    #[test]
    fn budget_and_bad_dimensions_are_rejected_before_strip_mutation() {
        assert!(
            Stitcher::new(Frame {
                width: 0,
                height: 256,
                bgra: Vec::new()
            })
            .is_err()
        );
        let mut value = Stitcher::new(frame(0, 256, 0, 0)).unwrap();
        value.rows = MAX_HEIGHT;
        assert_eq!(value.append(frame(80, 256, 0, 0)).unwrap(), Append::Limit);
        assert_eq!(value.parts(), 1);
        assert!(
            value
                .append(Frame {
                    width: 97,
                    height: 256,
                    bgra: vec![0; 97 * 256 * 4]
                })
                .is_err()
        );
    }

    fn text_frame(offset: u32, width: u32, height: u32) -> Frame {
        let mut bgra = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            let document_y = y + offset;
            let line = document_y / 32;
            let glyph_y = document_y % 32;
            for x in 0..width {
                let value = if (80..width - 40).contains(&x) && (10..23).contains(&glyph_y) {
                    let character = (x - 80) / 9;
                    let glyph_x = (x - 80) % 9;
                    let glyph = pixel(character, line * 17 + glyph_y / 2);
                    if glyph_x < 6 && glyph[0] & (1 << (glyph_x % 5)) != 0 {
                        42
                    } else {
                        250
                    }
                } else {
                    250
                };
                bgra.extend_from_slice(&[value, value, value, 255]);
            }
        }
        Frame {
            width,
            height,
            bgra,
        }
    }

    #[test]
    fn sparse_text_and_whitespace_stitch_without_collapsing_rows() {
        let mut value = Stitcher::new(text_frame(0, 640, 480)).unwrap();
        for (offset, added) in [(103, 103), (195, 92), (311, 116)] {
            assert_eq!(
                value.append(text_frame(offset, 640, 480)).unwrap(),
                Append::Added(added)
            );
        }
        let result = value.finish();
        assert_eq!(result.bgra, text_frame(0, 640, 791).bgra);
    }

    #[test]
    fn returning_from_backwards_scroll_only_appends_beyond_the_frontier() {
        let mut value = Stitcher::new(text_frame(0, 640, 480)).unwrap();
        assert_eq!(
            value.append(text_frame(120, 640, 480)).unwrap(),
            Append::Added(120)
        );
        assert_eq!(
            value.append(text_frame(70, 640, 480)).unwrap(),
            Append::Covered
        );
        assert_eq!(
            value.append(text_frame(10, 640, 480)).unwrap(),
            Append::Covered
        );
        assert_eq!(
            value.append(text_frame(150, 640, 480)).unwrap(),
            Append::Added(30)
        );
        assert_eq!(value.finish().bgra, text_frame(0, 640, 630).bgra);
    }

    #[test]
    fn animated_sidebar_cannot_overrule_scrolling_text() {
        let mut first = text_frame(0, 960, 600);
        let mut next = text_frame(153, 960, 600);
        for y in 0..600 {
            for x in 0..150 {
                let at = (y * 960 + x) as usize * 4;
                first.bgra[at..at + 4].copy_from_slice(&pixel(x, y + 700_000));
                next.bgra[at..at + 4].copy_from_slice(&pixel(x, y + 800_000));
            }
        }
        let mut value = Stitcher::new(first).unwrap();
        assert_eq!(value.append(next).unwrap(), Append::Added(153));
    }

    #[test]
    fn large_scroll_with_fixed_sidebar_uses_the_available_overlap() {
        let mut first = frame(0, 256, 0, 0);
        let mut next = frame(176, 256, 0, 0);
        for y in 0..256 {
            for x in 0..24 {
                let at = (y * 96 + x) as usize * 4;
                let fixed = pixel(x, y + 700_000);
                first.bgra[at..at + 4].copy_from_slice(&fixed);
                next.bgra[at..at + 4].copy_from_slice(&fixed);
            }
        }
        let mut value = Stitcher::new(first).unwrap();
        assert_eq!(value.append(next).unwrap(), Append::Added(176));
        let result = value.finish();
        assert_eq!(result.height, 432);
        let expected = frame(0, 432, 0, 0);
        for (actual, expected) in result
            .bgra
            .chunks_exact(96 * 4)
            .zip(expected.bgra.chunks_exact(96 * 4))
        {
            assert_eq!(&actual[24 * 4..], &expected[24 * 4..]);
        }
    }

    #[test]
    fn repeated_identical_list_items_are_rejected_instead_of_guessing_a_period() {
        let mut first = text_frame(0, 640, 480);
        let row_bytes = 640 * 4;
        let period = first.bgra[..32 * row_bytes].to_vec();
        for row in first.bgra.chunks_exact_mut(32 * row_bytes) {
            row.copy_from_slice(&period);
        }
        let mut shifted = first.bgra[13 * row_bytes..].to_vec();
        shifted.extend_from_slice(&first.bgra[..13 * row_bytes]);
        let mut value = Stitcher::new(first).unwrap();
        assert!(matches!(
            value
                .append(Frame {
                    width: 640,
                    height: 480,
                    bgra: shifted
                })
                .unwrap(),
            Append::NoOverlap | Append::Ambiguous
        ));
        assert_eq!(value.height(), 480);
    }

    #[test]
    fn dark_blank_edges_with_unaligned_width_are_not_mistaken_for_fixed_bars() {
        let frame = Frame {
            width: 333,
            height: 240,
            bgra: [30, 30, 30, 255].repeat(333 * 240),
        };
        assert_eq!(fixed_edges(&frame.bgra, &frame.bgra, 333, 240, 12), (0, 0));
    }

    #[test]
    fn smooth_scroll_is_bounded_by_pixels_instead_of_capture_count() {
        let mut value = Stitcher::new(frame(0, 128, 0, 0)).unwrap();
        for offset in 1..=300 {
            assert_eq!(
                value.append(frame(offset, 128, 0, 0)).unwrap(),
                Append::Added(1)
            );
        }
        assert!(value.parts() < 10);
        assert!(value.undo());
        assert_eq!(value.height(), 128 + 256);
        assert_eq!(
            value.append(frame(300, 128, 0, 0)).unwrap(),
            Append::Added(44)
        );
        assert_eq!(value.finish().bgra, frame(0, 428, 0, 0).bgra);
    }

    #[test]
    fn rejected_frame_does_not_move_the_match_anchor_or_frontier() {
        let mut value = Stitcher::new(text_frame(0, 640, 480)).unwrap();
        value.append(text_frame(103, 640, 480)).unwrap();
        let unrelated = Frame {
            width: 640,
            height: 480,
            bgra: [20, 40, 70, 255].repeat(640 * 480),
        };
        assert!(matches!(
            value.append(unrelated).unwrap(),
            Append::NoOverlap | Append::Ambiguous
        ));
        assert_eq!(value.observed_offset, 103);
        assert_eq!(value.max_offset, 103);
        assert_eq!(
            value.append(text_frame(195, 640, 480)).unwrap(),
            Append::Added(92)
        );
    }

    #[test]
    fn upward_capture_keeps_fixed_bars_once_and_thumbnail_in_document_order() {
        let mut value = Stitcher::new(frame(600, 256, 32, 24)).unwrap();
        assert_eq!(
            value.append(frame(520, 256, 32, 24)).unwrap(),
            Append::Added(80)
        );
        assert_eq!(
            value.append(frame(450, 256, 32, 24)).unwrap(),
            Append::Added(70)
        );
        let preview = value.thumbnail();
        let result = value.finish();
        assert_eq!(result.height, 406);
        assert_eq!(result.bgra, frame(450, 406, 32, 24).bgra);
        assert_eq!(preview.bgra, result.bgra);
    }

    #[test]
    fn alternating_extensions_undo_in_capture_order_and_restore_either_anchor() {
        let viewport = 384;
        let mut value = Stitcher::new(frame(500, viewport, 32, 24)).unwrap();
        for (offset, expected) in [
            (420, Append::Added(80)),
            (560, Append::Added(60)),
            (390, Append::Added(30)),
            (480, Append::Covered),
            (350, Append::Added(40)),
        ] {
            assert_eq!(
                value.append(frame(offset, viewport, 32, 24)).unwrap(),
                expected
            );
        }
        assert_eq!(value.height(), viewport + 210);
        assert_eq!(value.parts(), 4);
        for (offset, height) in [(420, viewport + 140), (500, viewport + 80), (500, viewport)] {
            assert!(value.undo());
            assert_eq!(value.height(), height);
            assert_eq!(value.previous, frame(offset, viewport, 32, 24).bgra);
        }
        assert!(!value.can_undo());
        assert!(!value.undo());
        assert_eq!(
            value.append(frame(430, viewport, 32, 24)).unwrap(),
            Append::Added(70)
        );
        assert_eq!(value.finish().bgra, frame(430, viewport + 70, 32, 24).bgra);
    }

    #[test]
    fn smooth_upward_scroll_merges_small_strips_and_can_resume_after_undo() {
        let mut value = Stitcher::new(frame(500, 128, 0, 0)).unwrap();
        for offset in (200..500).rev() {
            assert_eq!(
                value.append(frame(offset, 128, 0, 0)).unwrap(),
                Append::Added(1)
            );
        }
        assert!(value.parts() < 10);
        assert!(value.undo());
        assert_eq!(value.height(), 384);
        assert_eq!(value.previous, frame(244, 128, 0, 0).bgra);
        assert_eq!(
            value.append(frame(200, 128, 0, 0)).unwrap(),
            Append::Added(44)
        );
        assert_eq!(value.finish().bgra, frame(200, 428, 0, 0).bgra);
    }

    #[test]
    fn upward_limit_preserves_range_anchor_and_undo_history() {
        let mut value = Stitcher::new(frame(500, 256, 0, 0)).unwrap();
        value.rows = MAX_HEIGHT;
        assert_eq!(value.append(frame(420, 256, 0, 0)).unwrap(), Append::Limit);
        assert_eq!(
            (value.min_offset, value.max_offset, value.observed_offset),
            (0, 0, 0)
        );
        assert_eq!(value.previous, frame(500, 256, 0, 0).bgra);
        assert!(!value.can_undo());
    }

    #[test]
    fn rejected_frame_after_upward_scroll_does_not_move_either_boundary() {
        let mut value = Stitcher::new(frame(600, 256, 0, 0)).unwrap();
        assert_eq!(
            value.append(frame(480, 256, 0, 0)).unwrap(),
            Append::Added(120)
        );
        assert_eq!(
            value.append(frame(2000, 256, 0, 0)).unwrap(),
            Append::NoOverlap
        );
        assert_eq!(
            (value.min_offset, value.max_offset, value.observed_offset),
            (-120, 0, -120)
        );
        assert_eq!(
            value.append(frame(430, 256, 0, 0)).unwrap(),
            Append::Added(50)
        );
        assert_eq!(value.finish().bgra, frame(430, 426, 0, 0).bgra);
    }

    #[test]
    fn detail_preview_follows_both_edges_and_undo_without_changing_pixels() {
        let mut value = Stitcher::new(frame(600, 384, 32, 24)).unwrap();
        assert_eq!(
            value.append(frame(520, 384, 32, 24)).unwrap(),
            Append::Added(80)
        );
        assert_eq!(
            value.append(frame(650, 384, 32, 24)).unwrap(),
            Append::Added(50)
        );
        let check = |value: &Stitcher, at_bottom: bool| {
            let (preview, (start, end)) = value.detail_preview(96, 128);
            assert_eq!(end - start, 128);
            assert_eq!(preview.width, 96);
            assert_eq!(preview.height, 128);
            if at_bottom {
                assert_eq!(end, value.height());
            } else {
                assert_eq!(start, 0);
            }
            let mut original = value.header.clone();
            for strip in &value.strips {
                original.extend_from_slice(strip);
            }
            original.extend_from_slice(&value.footer);
            assert_eq!(
                preview.bgra,
                original[start as usize * 384..end as usize * 384]
            );
        };
        check(&value, true);
        assert_eq!(
            value.append(frame(600, 384, 32, 24)).unwrap(),
            Append::Covered
        );
        let (_, middle) = value.detail_preview(96, 128);
        assert!(middle.0 > 0 && middle.1 < value.height());
        assert_eq!(
            value.append(frame(480, 384, 32, 24)).unwrap(),
            Append::Added(40)
        );
        check(&value, false);
        assert!(value.undo());
        check(&value, false);
        assert!(value.undo());
        check(&value, true);
    }

    #[test]
    fn detail_preview_is_bounded_and_does_not_shrink_with_document_height() {
        for height in [800, 24_000] {
            let value = Stitcher::new(text_frame(0, 320, height)).unwrap();
            let (preview, range) = value.detail_preview(208, 224);
            assert_eq!((preview.width, preview.height), (208, 224));
            assert_eq!(range, (0, 344));
            assert!(preview.bgra.len() <= 208 * 224 * 4);
        }
        let value = Stitcher::new(frame(0, 96, 0, 0)).unwrap();
        let (preview, range) = value.detail_preview(416, 448);
        assert_eq!(range, (0, 96));
        assert_eq!(preview.bgra, value.previous);
    }

    #[test]
    fn optimized_text_stitch_latency() {
        if std::env::var_os("NCD_STITCH_BENCH").is_none() {
            return;
        }
        let mut value = Stitcher::new(text_frame(0, 1920, 1080)).unwrap();
        let mut times = Vec::new();
        for step in 1..=12 {
            let next = text_frame(step * 137, 1920, 1080);
            let start = std::time::Instant::now();
            assert_eq!(value.append(next).unwrap(), Append::Added(137));
            times.push(start.elapsed());
        }
        times.sort();
        println!(
            "1920x1080 text matching: median={:?}, max={:?}, 12 frames",
            times[times.len() / 2],
            times.last().unwrap()
        );
    }
}
