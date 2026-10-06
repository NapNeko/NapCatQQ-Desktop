// lucide 图标：画板是 24 单位，路径数据由 node 脚本从 lucide-react 0.469 导出
// （tmp/native/icons.mjs，不进仓库）。每个图标是该脚本输出里的 &[...] 一段，
// 描边宽度调用方按画板单位给，和 SVG 的 strokeWidth 一致；play / square 在前端带 fill。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    BatteryCharging,
    ChevronLeft,
    ChevronRight,
    Globe,
    LoaderCircle,
    LogOut,
    Monitor,
    PanelsTopLeft,
    Play,
    Square,
    Bell,
    Check,
    EyeOff,
    MessageCircle,
    Radio,
    Users,
    X,
}

pub fn paths(icon: Icon) -> &'static [&'static str] {
    match icon {
        Icon::BatteryCharging => &[
            "M15 7h1a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2h-2",
            "M6 7H4a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h1",
            "m11 7-3 5h4l-3 5",
            "M22 11L22 13",
        ],
        Icon::ChevronLeft => &["m15 18-6-6 6-6"],
        Icon::ChevronRight => &["m9 18 6-6-6-6"],
        Icon::Globe => &[
            "M2 12a10 10 0 1 0 20 0a10 10 0 1 0 -20 0",
            "M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20",
            "M2 12h20",
        ],
        Icon::LoaderCircle => &["M21 12a9 9 0 1 1-6.219-8.56"],
        Icon::LogOut => &[
            "M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4",
            "M16 17L21 12L16 7",
            "M21 12L9 12",
        ],
        Icon::Monitor => &[
            "M4 3h16a2 2 0 0 1 2 2v10a2 2 0 0 1 -2 2h-16a2 2 0 0 1 -2 -2v-10a2 2 0 0 1 2 -2Z",
            "M8 21L16 21",
            "M12 17L12 21",
        ],
        Icon::PanelsTopLeft => &[
            "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1 -2 2h-14a2 2 0 0 1 -2 -2v-14a2 2 0 0 1 2 -2Z",
            "M3 9h18",
            "M9 21V9",
        ],
        Icon::Play => &["M6 3L20 12L6 21L6 3Z"],
        Icon::Square => {
            &["M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1 -2 2h-14a2 2 0 0 1 -2 -2v-14a2 2 0 0 1 2 -2Z"]
        }
        Icon::Bell => &[
            "M10.268 21a2 2 0 0 0 3.464 0",
            "M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326",
        ],
        Icon::Check => &["M20 6 9 17l-5-5"],
        Icon::EyeOff => &[
            "M10.733 5.076a10.744 10.744 0 0 1 11.205 6.575 1 1 0 0 1 0 .696 10.747 10.747 0 0 1-1.444 2.49",
            "M14.084 14.158a3 3 0 0 1-4.242-4.242",
            "M17.479 17.499a10.75 10.75 0 0 1-15.417-5.151 1 1 0 0 1 0-.696 10.75 10.75 0 0 1 4.446-5.143",
            "m2 2 20 20",
        ],
        Icon::MessageCircle => &["M7.9 20A9 9 0 1 0 4 16.1L2 22Z"],
        Icon::Radio => &[
            "M4.9 19.1C1 15.2 1 8.8 4.9 4.9",
            "M7.8 16.2c-2.3-2.3-2.3-6.1 0-8.5",
            "M10 12a2 2 0 1 0 4 0a2 2 0 1 0 -4 0",
            "M16.2 7.8c2.3 2.3 2.3 6.1 0 8.5",
            "M19.1 4.9C23 8.8 23 15.1 19.1 19",
        ],
        Icon::Users => &[
            "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2",
            "M5 7a4 4 0 1 0 8 0a4 4 0 1 0 -8 0",
            "M22 21v-2a4 4 0 0 0-3-3.87",
            "M16 3.13a4 4 0 0 1 0 7.75",
        ],
        Icon::X => &["M18 6 6 18", "m6 6 12 12"],
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    Move((f32, f32)),
    Line((f32, f32)),
    Cubic((f32, f32), (f32, f32), (f32, f32)),
    Arc {
        to: (f32, f32),
        r: (f32, f32),
        rotation: f32,
        large: bool,
        sweep: bool,
    },
    Close,
}

/// 解析一段 SVG path 数据。图标集里只出现 M/L/H/V/C/A/Z（大小写都算），
/// 遇到别的命令或数字断流直接停掉，返回已解析到的部分，图标可能缺一段但结构安全。
pub fn parse(d: &str) -> Vec<Seg> {
    parse_inner(d).unwrap_or_default()
}

fn parse_inner(d: &str) -> Option<Vec<Seg>> {
    let bytes = d.as_bytes();
    let mut pos = 0usize;
    let mut out = Vec::new();
    let mut cmd = 0u8;
    let mut cur = (0.0f32, 0.0f32);
    let mut start = (0.0f32, 0.0f32);

    fn num(bytes: &[u8], pos: &mut usize) -> Option<f32> {
        while *pos < bytes.len() && matches!(bytes[*pos], b' ' | b',' | b'\t' | b'\n' | b'\r') {
            *pos += 1;
        }
        let begin = *pos;
        while *pos < bytes.len()
            && (bytes[*pos].is_ascii_digit()
                || matches!(bytes[*pos], b'+' | b'-' | b'.' | b'e' | b'E'))
        {
            // '-' 在新算式里是数字开头、在指数里也是，但前一个数是完整数字时它是下一个数的符号，
            // 扫描到带符号的指数位时不能把已结尾的数字的 '-' 吃进来
            if matches!(bytes[*pos], b'+' | b'-')
                && *pos > begin
                && !matches!(bytes[*pos - 1], b'e' | b'E')
            {
                break;
            }
            *pos += 1;
        }
        if begin == *pos {
            return None;
        }
        std::str::from_utf8(&bytes[begin..*pos]).ok()?.parse().ok()
    }

    while pos < bytes.len() {
        // 数字和符号轮次由下面的命令分支自己消耗；字母换新命令
        if bytes[pos].is_ascii_alphabetic() {
            cmd = bytes[pos];
            pos += 1;
            if matches!(cmd, b'z' | b'Z') {
                out.push(Seg::Close);
                cur = start;
            }
            continue;
        }
        let lower = cmd.to_ascii_lowercase();
        let rel = cmd.is_ascii_lowercase();
        let point = |x: f32, y: f32| if rel { (cur.0 + x, cur.1 + y) } else { (x, y) };
        match lower {
            b'm' => {
                let (x, y) = (num(bytes, &mut pos)?, num(bytes, &mut pos)?);
                cur = point(x, y);
                out.push(Seg::Move(cur));
                start = cur;
                // m 后面的坐标对按 line 处理（隐含 lineto）
                cmd = if rel { b'l' } else { b'L' };
            }
            b'l' => {
                let (x, y) = (num(bytes, &mut pos)?, num(bytes, &mut pos)?);
                cur = point(x, y);
                out.push(Seg::Line(cur));
            }
            b'h' => {
                let x = num(bytes, &mut pos)?;
                cur.0 = if rel { cur.0 + x } else { x };
                out.push(Seg::Line(cur));
            }
            b'v' => {
                let y = num(bytes, &mut pos)?;
                cur.1 = if rel { cur.1 + y } else { y };
                out.push(Seg::Line(cur));
            }
            b'c' => {
                let (x1, y1) = (num(bytes, &mut pos)?, num(bytes, &mut pos)?);
                let (x2, y2) = (num(bytes, &mut pos)?, num(bytes, &mut pos)?);
                let (x, y) = (num(bytes, &mut pos)?, num(bytes, &mut pos)?);
                let (p1, p2) = (point(x1, y1), point(x2, y2));
                cur = point(x, y);
                out.push(Seg::Cubic(p1, p2, cur));
            }
            b's' => {
                let (x2, y2) = (num(bytes, &mut pos)?, num(bytes, &mut pos)?);
                let (x, y) = (num(bytes, &mut pos)?, num(bytes, &mut pos)?);
                // 前一个控制点关于当前点镜像；上一段不是曲线就用当前点
                let p1 = match out.last() {
                    Some(Seg::Cubic(_, _, to)) => (2.0 * to.0 - 0.0, 2.0 * to.1 - 0.0),
                    _ => cur,
                };
                let p1 = if let Some(Seg::Cubic(_, cp2, to)) = out.last().copied() {
                    (2.0 * to.0 - cp2.0, 2.0 * to.1 - cp2.1)
                } else {
                    p1
                };
                let p2 = point(x2, y2);
                cur = point(x, y);
                out.push(Seg::Cubic(p1, p2, cur));
            }
            b'a' => {
                let (rx, ry) = (num(bytes, &mut pos)?, num(bytes, &mut pos)?);
                let rotation = num(bytes, &mut pos)?;
                let large = num(bytes, &mut pos)? != 0.0;
                let sweep = num(bytes, &mut pos)? != 0.0;
                let (x, y) = (num(bytes, &mut pos)?, num(bytes, &mut pos)?);
                cur = point(x, y);
                out.push(Seg::Arc {
                    to: cur,
                    r: (rx, ry),
                    rotation,
                    large,
                    sweep,
                });
            }
            _ => break,
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_relative_line_and_close() {
        // SVG 规定 m 之后的隐式 lineto 也按相对坐标
        let segs = parse("m2 2 20 20");
        assert_eq!(segs, vec![Seg::Move((2.0, 2.0)), Seg::Line((22.0, 22.0))]);
        let segs = parse("M6 3L20 12L6 21L6 3Z");
        assert_eq!(segs.len(), 5);
        assert!(matches!(segs.last(), Some(Seg::Close)));
    }

    #[test]
    fn parses_compact_numbers_and_arcs() {
        let segs = parse("M2 12a10 10 0 1 0 20 0a10 10 0 1 0 -20 0");
        assert_eq!(segs.len(), 3);
        let Some(Seg::Arc {
            to,
            r,
            large,
            sweep,
            ..
        }) = segs.get(1).copied()
        else {
            panic!("第二段应是圆弧: {segs:?}");
        };
        assert_eq!(to, (22.0, 12.0));
        assert_eq!(r, (10.0, 10.0));
        // flag 顺序是 large-arc 再 sweep：「1 0」= 大弧、逆时针
        assert!(large && !sweep);
    }

    #[test]
    fn parses_smooth_cubic_after_cubic() {
        // Bell 的 A...C 之后带一段普通 C；负号紧贴数字是常见压缩写法
        let segs = parse("M1 1C2.498 5.42 5.532 4.685 11.999 3.5");
        assert!(matches!(segs[1], Seg::Cubic(_, _, (11.999, 3.5))));
    }

    #[test]
    fn every_icon_path_parses() {
        for icon in [
            Icon::BatteryCharging,
            Icon::ChevronLeft,
            Icon::ChevronRight,
            Icon::Globe,
            Icon::LoaderCircle,
            Icon::LogOut,
            Icon::Monitor,
            Icon::PanelsTopLeft,
            Icon::Play,
            Icon::Square,
            Icon::Bell,
            Icon::Check,
            Icon::EyeOff,
            Icon::MessageCircle,
            Icon::Radio,
            Icon::Users,
            Icon::X,
        ] {
            for d in paths(icon) {
                assert!(!parse(d).is_empty(), "{icon:?} 路径没解出来: {d}");
            }
        }
    }
}
