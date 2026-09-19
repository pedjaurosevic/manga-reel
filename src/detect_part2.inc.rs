#[derive(Clone, Copy)]
struct Region {
    x0: usize,
    y0: usize,
    x1: usize, // exclusive
    y1: usize,
}

impl Region {
    fn width(self) -> usize {
        self.x1.saturating_sub(self.x0)
    }
    fn height(self) -> usize {
        self.y1.saturating_sub(self.y0)
    }
    fn area(self) -> usize {
        self.width().saturating_mul(self.height())
    }
}

fn bsp_split(
    gutter: &[bool],
    w: usize,
    h: usize,
    mut reg: Region,
    depth: u32,
    min_leaf: usize,
    splits: &mut u32,
    out: &mut Vec<(usize, usize, usize, usize)>,
) {
    if reg.width() < 4 || reg.height() < 4 {
        return;
    }
    if depth >= MAX_DEPTH || *splits >= MAX_SPLITS {
        if reg.area() >= min_leaf {
            out.push((reg.x0, reg.y0, reg.width(), reg.height()));
        }
        return;
    }

    // Trim empty top rows / left columns (and bottom / right for cleanliness).
    trim_empty_margins(gutter, w, &mut reg);
    if reg.width() < 4 || reg.height() < 4 {
        return;
    }

    if reg.area() <= min_leaf {
        out.push((reg.x0, reg.y0, reg.width(), reg.height()));
        return;
    }

    // Find first strong empty horizontal band OR vertical band inside region.
    let min_band_h = ((reg.height() as f32) * MIN_BAND_FRAC)
        .max(MIN_BAND_PX as f32) as usize;
    let min_band_w = ((reg.width() as f32) * MIN_BAND_FRAC)
        .max(MIN_BAND_PX as f32) as usize;

    let h_band = find_empty_band(gutter, w, reg, true, min_band_h);
    let v_band = find_empty_band(gutter, w, reg, false, min_band_w);

    // Prefer the band that appears first in reading order (topmost row-band,
    // else leftmost col-band), matching Comic Trim's "first strong empty".
    let split = match (h_band, v_band) {
        (Some(hb), Some(vb)) => {
            // Prefer horizontal if it starts earlier (smaller y) than vertical's x
            // relative to region — Comic Trim tries H then V; we pick the one
            // whose start is closer to the trimmed origin along its axis.
            let h_rel = hb.0 - reg.y0;
            let v_rel = vb.0 - reg.x0;
            if h_rel <= v_rel {
                Some((true, hb))
            } else {
                Some((false, vb))
            }
        }
        (Some(hb), None) => Some((true, hb)),
        (None, Some(vb)) => Some((false, vb)),
        (None, None) => None,
    };

    let Some((horizontal, (b0, b1))) = split else {
        // No internal gutter band — this region is one panel.
        if reg.area() >= min_leaf {
            out.push((reg.x0, reg.y0, reg.width(), reg.height()));
        }
        return;
    };

    *splits += 1;

    if horizontal {
        // Split above / below the empty row band.
        let top = Region {
            x0: reg.x0,
            y0: reg.y0,
            x1: reg.x1,
            y1: b0,
        };
        let bot = Region {
            x0: reg.x0,
            y0: b1,
            x1: reg.x1,
            y1: reg.y1,
        };
        if top.area() >= min_leaf / 2 {
            bsp_split(gutter, w, h, top, depth + 1, min_leaf, splits, out);
        }
        if bot.area() >= min_leaf / 2 {
            bsp_split(gutter, w, h, bot, depth + 1, min_leaf, splits, out);
        }
    } else {
        let left = Region {
            x0: reg.x0,
            y0: reg.y0,
            x1: b0,
            y1: reg.y1,
        };
        let right = Region {
            x0: b1,
            y0: reg.y0,
            x1: reg.x1,
            y1: reg.y1,
        };
        if left.area() >= min_leaf / 2 {
            bsp_split(gutter, w, h, left, depth + 1, min_leaf, splits, out);
        }
        if right.area() >= min_leaf / 2 {
            bsp_split(gutter, w, h, right, depth + 1, min_leaf, splits, out);
        }
    }
}

fn trim_empty_margins(gutter: &[bool], w: usize, reg: &mut Region) {
    // Top
    while reg.y0 < reg.y1 && row_is_gutter(gutter, w, reg, reg.y0) {
        reg.y0 += 1;
    }
    // Bottom
    while reg.y1 > reg.y0 && row_is_gutter(gutter, w, reg, reg.y1 - 1) {
        reg.y1 -= 1;
    }
    // Left
    while reg.x0 < reg.x1 && col_is_gutter(gutter, w, reg, reg.x0) {
        reg.x0 += 1;
    }
    // Right
    while reg.x1 > reg.x0 && col_is_gutter(gutter, w, reg, reg.x1 - 1) {
        reg.x1 -= 1;
    }
}

fn row_is_gutter(gutter: &[bool], w: usize, reg: &Region, y: usize) -> bool {
    let mut g = 0u32;
    let n = reg.width() as u32;
    if n == 0 {
        return true;
    }
    for x in reg.x0..reg.x1 {
        if gutter[y * w + x] {
            g += 1;
        }
    }
    g as f32 / n as f32 >= BAND_GUTTER_FRAC
}

fn col_is_gutter(gutter: &[bool], w: usize, reg: &Region, x: usize) -> bool {
    let mut g = 0u32;
    let n = reg.height() as u32;
    if n == 0 {
        return true;
    }
    for y in reg.y0..reg.y1 {
        if gutter[y * w + x] {
            g += 1;
        }
    }
    g as f32 / n as f32 >= BAND_GUTTER_FRAC
}

/// Find first contiguous empty band (rows if `horizontal`, else cols) that is
/// thick enough and not fragmented. Returns inclusive-exclusive [b0, b1).
fn find_empty_band(
    gutter: &[bool],
    w: usize,
    reg: Region,
    horizontal: bool,
    min_thick: usize,
) -> Option<(usize, usize)> {
    let (start, end) = if horizontal {
        (reg.y0, reg.y1)
    } else {
        (reg.x0, reg.x1)
    };
    if end <= start + min_thick + 2 {
        return None;
    }

    // Skip leading margin gutters (already trimmed, but be safe).
    let mut i = start;
    while i < end
        && if horizontal {
            row_is_gutter(gutter, w, &reg, i)
        } else {
            col_is_gutter(gutter, w, &reg, i)
        }
    {
        i += 1;
    }

    while i < end {
        let is_g = if horizontal {
            row_is_gutter(gutter, w, &reg, i)
        } else {
            col_is_gutter(gutter, w, &reg, i)
        };
        if !is_g {
            i += 1;
            continue;
        }
        let b0 = i;
        while i < end
            && if horizontal {
                row_is_gutter(gutter, w, &reg, i)
            } else {
                col_is_gutter(gutter, w, &reg, i)
            }
        {
            i += 1;
        }
        let b1 = i;
        // Must be internal (content on both sides) and thick enough.
        if b1 - b0 >= min_thick && b0 > start && b1 < end {
            return Some((b0, b1));
        }
    }
    None
}
