// 连续内容模板提出位移，完整运动区域与原始像素共同确认拼接位置。
const MAX_COLUMNS: usize = 256;
const BANDS: usize = 4;
const TEMPLATE_HEIGHT: usize = 24;

struct Samples {
    width: usize,
    rows: Vec<u8>,
    edges: Vec<u16>,
}

impl Samples {
    fn new(pixels: &[u8], width: usize, height: usize) -> Self {
        let columns = width.min(MAX_COLUMNS);
        let mut rows = Vec::with_capacity(columns * height);
        for y in 0..height {
            for column in 0..columns {
                let left = column * width / columns;
                let right = ((column + 1) * width / columns).max(left + 1);
                let mut sum = 0u32;
                let count = (right - left).min(3);
                for sample in 0..count {
                    let x = left + (sample * 2 + 1) * (right - left) / (count * 2);
                    let at = (y * width + x) * 4;
                    sum += (29 * u32::from(pixels[at])
                        + 150 * u32::from(pixels[at + 1])
                        + 77 * u32::from(pixels[at + 2]))
                        >> 8;
                }
                rows.push((sum / count as u32) as u8);
            }
        }
        let mut edges = Vec::with_capacity(rows.len());
        for y in 0..height {
            let row = y * columns;
            let above = y.saturating_sub(1) * columns;
            for x in 0..columns {
                let value = rows[row + x];
                edges.push(
                    u16::from(value.abs_diff(rows[row + x.saturating_sub(1)]))
                        + u16::from(value.abs_diff(rows[above + x])),
                );
            }
        }
        Self {
            width: columns,
            rows,
            edges,
        }
    }

    #[inline(always)]
    fn at(&self, x: usize, y: usize) -> u8 {
        self.rows[y * self.width + x]
    }

    fn band(&self, band: usize) -> (usize, usize) {
        (band * self.width / BANDS, (band + 1) * self.width / BANDS)
    }

    #[inline(always)]
    fn edge(&self, x: usize, y: usize) -> u32 {
        u32::from(self.edges[y * self.width + x])
    }
}

fn moving_extent(a: &Samples, b: &Samples, begin: usize, end: usize) -> (usize, usize) {
    let boundary = |rows: &mut dyn Iterator<Item = usize>| {
        let mut count = 0;
        let mut textured = false;
        for y in rows {
            let first = &a.rows[y * a.width..(y + 1) * a.width];
            let second = &b.rows[y * b.width..(y + 1) * b.width];
            if first.iter().zip(second).any(|(&a, &b)| a.abs_diff(b) > 2) {
                break;
            }
            count += 1;
            textured |= first
                .windows(2)
                .filter(|pair| pair[0].abs_diff(pair[1]) >= 6)
                .take(3)
                .count()
                == 3;
        }
        if textured { count } else { 0 }
    };
    let limit = (end - begin) / 3;
    let top = boundary(&mut (begin..begin + limit));
    let bottom = boundary(&mut (end - limit..end).rev());
    (begin + top, end - bottom)
}

struct Template {
    y: usize,
    band: usize,
    energy: u32,
}

fn templates(a: &Samples, b: &Samples, begin: usize, end: usize) -> Vec<Template> {
    let mut result: Vec<Template> = Vec::new();
    for band in 0..BANDS {
        let (left, right) = a.band(band);
        let mut prefix = vec![0u64; end + 1];
        for y in begin..end {
            let mut score = 0u64;
            for x in left..right {
                let change = a.at(x, y).abs_diff(b.at(x, y));
                if change >= 2 {
                    score += u64::from(a.edge(x, y).min(128)) * u64::from(change.min(32));
                }
            }
            prefix[y + 1] = prefix[y] + score;
        }
        let last = end - TEMPLATE_HEIGHT;
        for part in 0..4 {
            let start = begin + part * (last - begin + 1) / 4;
            let stop = begin + (part + 1) * (last - begin + 1) / 4;
            let Some(y) = (start..stop).max_by_key(|&y| prefix[y + TEMPLATE_HEIGHT] - prefix[y])
            else {
                continue;
            };
            if prefix[y + TEMPLATE_HEIGHT] == prefix[y]
                || result
                    .iter()
                    .any(|t| t.band == band && t.y.abs_diff(y) < TEMPLATE_HEIGHT / 2)
            {
                continue;
            }
            let mut energy = 0;
            for dy in (0..TEMPLATE_HEIGHT).step_by(3) {
                for x in (left..right).step_by(2) {
                    energy += a.edge(x, y + dy);
                }
            }
            if energy >= 32 {
                result.push(Template { y, band, energy });
            }
        }
    }
    result
}

fn template_error(a: &Samples, b: &Samples, template: &Template, y: usize, bound: u32) -> u32 {
    let (left, right) = a.band(template.band);
    let mut error = 0;
    for dy in (0..TEMPLATE_HEIGHT).step_by(3) {
        let previous =
            &a.rows[(template.y + dy) * a.width + left..(template.y + dy) * a.width + right];
        let current = &b.rows[(y + dy) * b.width + left..(y + dy) * b.width + right];
        for x in (0..right - left).step_by(2) {
            error += u32::from(previous[x].abs_diff(current[x]));
        }
        if error > bound {
            break;
        }
    }
    error
}

#[derive(Clone, Copy, Default)]
struct Candidate {
    shift: i32,
    votes: u32,
    error: u32,
}

#[derive(Default)]
struct Evidence {
    count: u32,
    supported: u32,
    aligned: u64,
    direct: u64,
    first_x: usize,
    last_x: usize,
    first_y: usize,
    last_y: usize,
}

impl Evidence {
    fn add(&mut self, x: usize, y: usize, aligned: u32, direct: u32) {
        if self.count == 0 {
            self.first_x = x;
            self.first_y = y;
        }
        self.first_x = self.first_x.min(x);
        self.last_x = self.last_x.max(x);
        self.last_y = y;
        self.count += 1;
        self.aligned += u64::from(aligned);
        self.direct += u64::from(direct);
        if aligned <= 3 || aligned * 4 <= direct + 8 {
            self.supported += 1;
        }
    }

    fn accepts(&self, minimum_width: usize) -> bool {
        self.count >= 16
            && self.last_x.saturating_sub(self.first_x) >= minimum_width
            && self.last_y.saturating_sub(self.first_y) >= 3
            && self.supported * 100 >= self.count * 70
            && self.aligned * 100 <= self.direct * 38
            && self.aligned <= u64::from(self.count) * 10
    }

    fn score(&self) -> f64 {
        self.aligned as f64 / self.direct.max(1) as f64
    }
}

fn overlap(begin: usize, end: usize, shift: i32) -> (usize, usize) {
    (
        begin.max((begin as i32 - shift).max(0) as usize),
        end.min((end as i32 - shift).max(0) as usize),
    )
}

struct SamplePoint {
    index: usize,
    y: u32,
    x: u16,
    direct: u8,
}

#[derive(Default)]
struct BandPoints {
    changed: Vec<SamplePoint>,
    stationary: Vec<usize>,
}

struct PixelPoint {
    index: usize,
    y: u32,
    x: u32,
    direct: u16,
    edge: u16,
    band: u8,
}

struct Verification {
    bands: [BandPoints; BANDS],
    pixels: Vec<PixelPoint>,
}

impl Verification {
    fn new(
        a: &Samples,
        b: &Samples,
        pixels_a: &[u8],
        pixels_b: &[u8],
        width: usize,
        begin: usize,
        end: usize,
    ) -> Self {
        let mut bands: [BandPoints; BANDS] = std::array::from_fn(|_| BandPoints::default());
        for y in begin..end {
            let middle = y >= begin + (end - begin) / 5 && y < end - (end - begin) / 5;
            for (band, points) in bands.iter_mut().enumerate() {
                let (left, right) = a.band(band);
                for x in left..right {
                    let index = y * a.width + x;
                    let direct = a.rows[index].abs_diff(b.rows[index]);
                    if direct >= 3 {
                        points.changed.push(SamplePoint {
                            index,
                            y: y as u32,
                            x: x as u16,
                            direct,
                        });
                    } else if middle && b.edges[index] >= 8 {
                        points.stationary.push(index);
                    }
                }
            }
        }
        let mut pixels = Vec::new();
        for y in begin..end {
            for x in (1..width).step_by((width / 512).max(1)) {
                let index = (y * width + x) * 4;
                let mut direct = 0u16;
                for channel in 0..3 {
                    direct +=
                        u16::from(pixels_a[index + channel].abs_diff(pixels_b[index + channel]));
                }
                if direct >= 12 {
                    let mut edge = 0u16;
                    for channel in 0..3 {
                        edge += u16::from(
                            pixels_b[index + channel].abs_diff(pixels_b[index + channel - 4]),
                        );
                    }
                    pixels.push(PixelPoint {
                        index,
                        y: y as u32,
                        x: x as u32,
                        direct,
                        edge,
                        band: 1 << (x * BANDS / width).min(BANDS - 1),
                    });
                }
            }
        }
        Self { bands, pixels }
    }
}

fn verify_samples(
    a: &Samples,
    b: &Samples,
    points: &Verification,
    candidate: Candidate,
    begin: usize,
    end: usize,
    content_height: usize,
) -> Option<(f64, u8)> {
    let (start, stop) = overlap(begin, end, candidate.shift);
    if stop <= start {
        return None;
    }
    let mut accepted = 0u8;
    let mut score = 0.0;
    let mut scored_bands = 0;
    let mut stationary_total = 0;
    let mut moving_first = end;
    let mut moving_last = 0;
    let first = start * a.width;
    let last = stop * a.width;
    let displacement = candidate.shift as isize * a.width as isize;
    for (band, points) in points.bands.iter().enumerate() {
        let mut evidence = Evidence::default();
        let mut stationary_content = 0;
        let from = points.stationary.partition_point(|&index| index < first);
        let to = points.stationary.partition_point(|&index| index < last);
        for &index in &points.stationary[from..to] {
            let old = (index as isize + displacement) as usize;
            if a.rows[old].abs_diff(b.rows[index]) >= 8 {
                stationary_content += 1;
            }
        }
        let from = points.changed.partition_point(|point| point.index < first);
        let to = points.changed.partition_point(|point| point.index < last);
        for point in &points.changed[from..to] {
            let old = (point.index as isize + displacement) as usize;
            if a.edges[old].max(b.edges[point.index]) < 4 {
                continue;
            }
            let error = u32::from(a.rows[old].abs_diff(b.rows[point.index]));
            evidence.add(
                point.x as usize,
                point.y as usize,
                error,
                u32::from(point.direct),
            );
        }
        stationary_total += stationary_content;
        // 不匹配的内容列也贡献惩罚；按列归一，避免动画侧栏的像素数压过正文。
        if evidence.accepts((a.width / 64).max(2)) && stationary_content <= evidence.count {
            accepted |= 1 << band;
            score += evidence.score();
            scored_bands += 1;
            moving_first = moving_first.min(evidence.first_y);
            moving_last = moving_last.max(evidence.last_y);
        } else if evidence.count >= 16 {
            score += 1.0;
            scored_bands += 1;
        }
    }
    // 验证裁剪可能包含静止正文，局部运动尺度仍以原始内容视口的重叠为准。
    let content_overlap = content_height.saturating_sub(candidate.shift.unsigned_abs() as usize);
    if accepted == 0
        || (stationary_total >= 64
            && moving_last.saturating_sub(moving_first) < content_overlap / 2)
    {
        return None;
    }
    Some((score / f64::from(scored_bands), accepted))
}

fn verify_pixels(
    a: &[u8],
    b: &[u8],
    points: &Verification,
    width: usize,
    begin: usize,
    end: usize,
    shift: i32,
    bands: u8,
) -> bool {
    let (start, stop) = overlap(begin, end, shift);
    let mut evidence = Evidence::default();
    let from = points
        .pixels
        .partition_point(|point| (point.y as usize) < start);
    let to = points
        .pixels
        .partition_point(|point| (point.y as usize) < stop);
    let displacement = shift as isize * width as isize * 4;
    for point in &points.pixels[from..to] {
        if bands & point.band == 0 {
            continue;
        }
        let old = (point.index as isize + displacement) as usize;
        let mut aligned = 0;
        let mut edge = u32::from(point.edge);
        for channel in 0..3 {
            aligned += u32::from(a[old + channel].abs_diff(b[point.index + channel]));
            edge += u32::from(a[old + channel].abs_diff(a[old + channel - 4]));
        }
        if edge >= 12 {
            evidence.add(
                point.x as usize,
                point.y as usize,
                aligned / 3,
                u32::from(point.direct) / 3,
            );
        }
    }
    evidence.accepts((width / 64).max(4)) && evidence.score() <= 0.38
}

pub(super) fn find_shift(
    a: &[u8],
    b: &[u8],
    width: u32,
    height: u32,
    top: u32,
    bottom: u32,
) -> (Option<i32>, bool) {
    let width = width as usize;
    let height = height as usize;
    let begin = (top as usize).max(1);
    let end = height.saturating_sub(bottom as usize);
    if end <= begin + TEMPLATE_HEIGHT + 8 {
        return (None, false);
    }
    #[cfg(test)]
    let profile = (width == 1920 && std::env::var_os("NCD_STITCH_PROFILE").is_some())
        .then(std::time::Instant::now);
    let previous = Samples::new(a, width, height);
    let current = Samples::new(b, width, height);
    #[cfg(test)]
    let samples_elapsed = profile.map(|start| start.elapsed());
    // 先估位移时还没有固定栏信息；校验时不能把旧页脚与新正文互相比较。
    let (verify_begin, verify_end) = moving_extent(&previous, &current, begin, end);
    let templates = templates(&previous, &current, begin, end);
    #[cfg(test)]
    let templates_elapsed = profile.map(|start| start.elapsed());
    if templates.is_empty() {
        return (Some(0), false);
    }
    let minimum_overlap = ((end - begin) / 8).clamp(TEMPLATE_HEIGHT + 4, 96);
    let maximum = ((end - begin) - minimum_overlap) as i32;
    let mut votes = vec![Candidate::default(); height * 2 + 1];
    for template in &templates {
        let bound = template.energy / 2 + 256;
        let mut errors = Vec::with_capacity(end - begin);
        let mut best = bound;
        for y in begin..=end - TEMPLATE_HEIGHT {
            let shift = template.y as i32 - y as i32;
            let error = if shift.abs() <= maximum {
                template_error(&previous, &current, template, y, bound)
            } else {
                u32::MAX
            };
            best = best.min(error);
            errors.push(error);
        }
        if best >= bound {
            continue;
        }
        let allowed = best + (template.energy / 24).max(32);
        for (row, error) in errors.into_iter().enumerate() {
            if error > allowed || error >= bound {
                continue;
            }
            let shift = template.y as i32 - (begin + row) as i32;
            let candidate = &mut votes[(shift + height as i32) as usize];
            candidate.shift = shift;
            candidate.votes += 1;
            candidate.error += error;
        }
    }
    let mut candidates: Vec<_> = votes
        .into_iter()
        .filter(|c| c.votes != 0 && c.shift != 0)
        .collect();
    candidates.sort_by(|a, b| {
        b.votes
            .cmp(&a.votes)
            .then((a.error * b.votes).cmp(&(b.error * a.votes)))
            .then(a.shift.abs().cmp(&b.shift.abs()))
    });
    #[cfg(test)]
    let search_elapsed = profile.map(|start| start.elapsed());
    let points = Verification::new(&previous, &current, a, b, width, verify_begin, verify_end);
    let mut tested: Vec<i32> = Vec::new();
    let mut verified = Vec::new();
    for candidate in candidates {
        if tested
            .iter()
            .any(|&shift| (candidate.shift - shift).abs() <= 1)
        {
            continue;
        }
        tested.push(candidate.shift);
        if let Some((error, bands)) = verify_samples(
            &previous,
            &current,
            &points,
            candidate,
            verify_begin,
            verify_end,
            end - begin,
        ) {
            verified.push((error, candidate, bands));
        }
        if tested.len() == 12 {
            break;
        }
    }
    verified.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1.votes.cmp(&a.1.votes)));
    let mut accepted = Vec::new();
    for &(error, candidate, bands) in &verified {
        if verify_pixels(
            a,
            b,
            &points,
            width,
            verify_begin,
            verify_end,
            candidate.shift,
            bands,
        ) {
            accepted.push((error, candidate));
        }
        if accepted.len() >= 2 {
            break;
        }
    }
    let Some(&(error, best)) = accepted.first() else {
        return (None, false);
    };
    let ambiguous = accepted.iter().skip(1).any(|(other_error, other)| {
        (other.shift - best.shift).abs() > 1 && *other_error <= error + 0.003 + error * 0.15
    });
    #[cfg(test)]
    if let (Some(start), Some(samples), Some(templates), Some(search)) =
        (profile, samples_elapsed, templates_elapsed, search_elapsed)
    {
        eprintln!(
            "stitch profile: samples={samples:?}, templates={:?}, search={:?}, verify={:?}, candidates={}",
            templates - samples,
            search - templates,
            start.elapsed() - search,
            tested.len()
        );
    }
    (Some(best.shift), ambiguous)
}
