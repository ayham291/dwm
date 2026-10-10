//! Layout algorithms ported from `vanitygaps.c` / classic dwm.

use crate::monitor::{Client, Gaps};

/// Axis-aligned integer rectangle in compositor coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn area(&self) -> i64 {
        (self.w.max(0) as i64) * (self.h.max(0) as i64)
    }

    pub fn contains_point(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutKind {
    Tile,
    BottomStack,
    Spiral,
    Dwindle,
    Deck,
    Monocle,
    CenteredMaster,
    CenteredFloatingMaster,
    Floating,
}

impl LayoutKind {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Tile => "[]=",
            Self::BottomStack => "TTT",
            Self::Spiral => "[@]",
            Self::Dwindle => "[\\]",
            Self::Deck => "[D]",
            Self::Monocle => "[M]",
            Self::CenteredMaster => "|M|",
            Self::CenteredFloatingMaster => ">M>",
            Self::Floating => "><>",
        }
    }

    pub fn all() -> &'static [LayoutKind] {
        &[
            Self::Tile,
            Self::BottomStack,
            Self::Spiral,
            Self::Dwindle,
            Self::Deck,
            Self::Monocle,
            Self::CenteredMaster,
            Self::CenteredFloatingMaster,
            Self::Floating,
        ]
    }

    pub fn next(self) -> Self {
        let all = Self::all();
        let idx = all.iter().position(|&k| k == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ArrangeParams {
    pub window_area: Rect,
    pub mfact: f32,
    pub nmaster: usize,
    pub gaps: Gaps,
    pub enable_gaps: bool,
    pub smart_gaps: bool,
    pub border_px: i32,
    pub layout: LayoutKind,
}

/// Compute geometries for non-floating clients. Returns (client_index_in_tiled, rect)
/// paired with an optional overridden layout symbol (deck uses `"D N"`).
pub fn arrange(
    tiled: &[&Client],
    params: ArrangeParams,
) -> (Vec<Rect>, String) {
    let n = tiled.len();
    let symbol = if params.layout == LayoutKind::Deck && n > params.nmaster {
        format!("D {}", n - params.nmaster)
    } else if params.layout == LayoutKind::Monocle && n > 0 {
        format!("[{n}]")
    } else {
        params.layout.symbol().to_string()
    };

    if n == 0 || params.layout == LayoutKind::Floating {
        return (Vec::new(), symbol);
    }

    let (oh, ov, ih, iv) = effective_gaps(&params, n);
    let bw = params.border_px;
    let wa = params.window_area;

    let rects = match params.layout {
        LayoutKind::Tile => tile(tiled, wa, params.mfact, params.nmaster, oh, ov, ih, iv, bw),
        LayoutKind::BottomStack => {
            bstack(tiled, wa, params.mfact, params.nmaster, oh, ov, ih, iv, bw)
        }
        LayoutKind::Deck => deck(tiled, wa, params.mfact, params.nmaster, oh, ov, ih, iv, bw),
        LayoutKind::Monocle => monocle(tiled, wa, oh, ov, bw),
        LayoutKind::Spiral => fibonacci(tiled, wa, params.mfact, oh, ov, ih, iv, bw, false),
        LayoutKind::Dwindle => fibonacci(tiled, wa, params.mfact, oh, ov, ih, iv, bw, true),
        LayoutKind::CenteredMaster => {
            centered_master(tiled, wa, params.mfact, params.nmaster, oh, ov, ih, iv, bw)
        }
        LayoutKind::CenteredFloatingMaster => {
            centered_floating_master(tiled, wa, params.mfact, params.nmaster, oh, ov, ih, iv, bw)
        }
        LayoutKind::Floating => Vec::new(),
    };

    (rects, symbol)
}

fn effective_gaps(params: &ArrangeParams, n: usize) -> (i32, i32, i32, i32) {
    let mut oe = params.enable_gaps;
    let ie = params.enable_gaps;
    if params.smart_gaps && n == 1 {
        oe = false;
    }
    let _oh = if oe { params.gaps.outer_h as i32 } else { 0 };
    let _ov = if oe { params.gaps.outer_v as i32 } else { 0 };
    let ih = if ie { params.gaps.inner_h as i32 } else { 0 };
    let _iv = if ie { params.gaps.inner_v as i32 } else { 0 };
    // Note: dwm's gappih is "horiz inner gap" used between master stack vertically
    // in tile — we keep the same field names as config.h.
    let _ = ih;
    (
        params.gaps.outer_h as i32 * oe as i32,
        params.gaps.outer_v as i32 * oe as i32,
        params.gaps.inner_h as i32 * ie as i32,
        params.gaps.inner_v as i32 * ie as i32,
    )
}

fn get_facts(n: usize, nmaster: usize, msize: i32, ssize: i32) -> (f32, f32, i32, i32) {
    let mfacts = (n.min(nmaster) as f32).max(0.0);
    let sfacts = ((n as isize - nmaster as isize).max(0) as f32).max(0.0);
    let mut mtotal = 0;
    let mut stotal = 0;
    for i in 0..n {
        if i < nmaster {
            if mfacts > 0.0 {
                mtotal += msize / mfacts as i32;
            }
        } else if sfacts > 0.0 {
            stotal += ssize / sfacts as i32;
        }
    }
    (mfacts, sfacts, msize - mtotal, ssize - stotal)
}

#[allow(clippy::too_many_arguments)]
fn tile(
    tiled: &[&Client],
    wa: Rect,
    mfact: f32,
    nmaster: usize,
    oh: i32,
    ov: i32,
    ih: i32,
    iv: i32,
    bw: i32,
) -> Vec<Rect> {
    let n = tiled.len();
    let mx = wa.x + ov;
    let mut my = wa.y + oh;
    let mut sx = mx;
    let mut sy = my;
    let mh = wa.h - 2 * oh - ih * ((n.min(nmaster) as i32) - 1).max(0);
    let sh = wa.h - 2 * oh - ih * ((n as i32 - nmaster as i32) - 1).max(0);
    let mut mw = wa.w - 2 * ov;
    let mut sw = mw;

    if nmaster > 0 && n > nmaster {
        sw = ((mw - iv) as f32 * (1.0 - mfact)) as i32;
        mw = ((mw - iv) as f32 * mfact) as i32;
        sx = mx + mw + iv;
    }

    let (mfacts, sfacts, mrest, srest) = get_facts(n, nmaster, mh, sh);
    let mut out = Vec::with_capacity(n);

    for (i, _) in tiled.iter().enumerate() {
        if i < nmaster {
            let h = if mfacts > 0.0 {
                (mh as f32 / mfacts) as i32 + if i < mrest as usize { 1 } else { 0 }
            } else {
                mh
            };
            out.push(Rect::new(mx, my, mw - 2 * bw, h - 2 * bw));
            my += h + ih;
        } else {
            let h = if sfacts > 0.0 {
                (sh as f32 / sfacts) as i32
                    + if (i - nmaster) < srest as usize {
                        1
                    } else {
                        0
                    }
            } else {
                sh
            };
            out.push(Rect::new(sx, sy, sw - 2 * bw, h - 2 * bw));
            sy += h + ih;
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn bstack(
    tiled: &[&Client],
    wa: Rect,
    mfact: f32,
    nmaster: usize,
    oh: i32,
    ov: i32,
    ih: i32,
    iv: i32,
    bw: i32,
) -> Vec<Rect> {
    let n = tiled.len();
    let mut mx = wa.x + ov;
    let my = wa.y + oh;
    let mut sx = mx;
    let mut sy = my;
    let mut mh = wa.h - 2 * oh;
    let mut sh = mh;
    let mw = wa.w - 2 * ov - iv * ((n.min(nmaster) as i32) - 1).max(0);
    let sw = wa.w - 2 * ov - iv * ((n as i32 - nmaster as i32) - 1).max(0);

    if nmaster > 0 && n > nmaster {
        sh = ((mh - ih) as f32 * (1.0 - mfact)) as i32;
        mh = ((mh - ih) as f32 * mfact) as i32;
        sx = mx;
        sy = my + mh + ih;
    }

    let (mfacts, sfacts, mrest, srest) = get_facts(n, nmaster, mw, sw);
    let mut out = Vec::with_capacity(n);

    for (i, _) in tiled.iter().enumerate() {
        if i < nmaster {
            let w = if mfacts > 0.0 {
                (mw as f32 / mfacts) as i32 + if i < mrest as usize { 1 } else { 0 }
            } else {
                mw
            };
            out.push(Rect::new(mx, my, w - 2 * bw, mh - 2 * bw));
            mx += w + iv;
        } else {
            let w = if sfacts > 0.0 {
                (sw as f32 / sfacts) as i32
                    + if (i - nmaster) < srest as usize {
                        1
                    } else {
                        0
                    }
            } else {
                sw
            };
            out.push(Rect::new(sx, sy, w - 2 * bw, sh - 2 * bw));
            sx += w + iv;
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn deck(
    tiled: &[&Client],
    wa: Rect,
    mfact: f32,
    nmaster: usize,
    oh: i32,
    ov: i32,
    ih: i32,
    iv: i32,
    bw: i32,
) -> Vec<Rect> {
    let n = tiled.len();
    let mx = wa.x + ov;
    let mut my = wa.y + oh;
    let sx;
    let sy = my;
    let mh = wa.h - 2 * oh - ih * ((n.min(nmaster) as i32) - 1).max(0);
    let mut sh = mh;
    let mut mw = wa.w - 2 * ov;
    let mut sw = mw;

    if nmaster > 0 && n > nmaster {
        sw = ((mw - iv) as f32 * (1.0 - mfact)) as i32;
        mw = ((mw - iv) as f32 * mfact) as i32;
        sx = mx + mw + iv;
        sh = wa.h - 2 * oh;
    } else {
        sx = mx;
    }

    let (mfacts, _sfacts, mrest, _srest) = get_facts(n, nmaster, mh, sh);
    let mut out = Vec::with_capacity(n);

    for (i, _) in tiled.iter().enumerate() {
        if i < nmaster {
            let h = if mfacts > 0.0 {
                (mh as f32 / mfacts) as i32 + if i < mrest as usize { 1 } else { 0 }
            } else {
                mh
            };
            out.push(Rect::new(mx, my, mw - 2 * bw, h - 2 * bw));
            my += h + ih;
        } else {
            out.push(Rect::new(sx, sy, sw - 2 * bw, sh - 2 * bw));
        }
    }
    out
}

fn monocle(tiled: &[&Client], wa: Rect, oh: i32, ov: i32, bw: i32) -> Vec<Rect> {
    tiled
        .iter()
        .map(|_| Rect::new(wa.x + ov, wa.y + oh, wa.w - 2 * ov - 2 * bw, wa.h - 2 * oh - 2 * bw))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn fibonacci(
    tiled: &[&Client],
    wa: Rect,
    mfact: f32,
    oh: i32,
    ov: i32,
    ih: i32,
    iv: i32,
    bw: i32,
    dwindle: bool,
) -> Vec<Rect> {
    let n = tiled.len();
    let mut nx = wa.x + ov;
    let mut ny = oh;
    let mut nw = wa.w - 2 * ov;
    let mut nh = wa.h - 2 * oh;
    let mut out = Vec::with_capacity(n);
    let mut i = 0usize;

    for c in tiled {
        if (i % 2 == 1 && nh / 2 > 2 * c.border)
            || (i % 2 == 0 && nw / 2 > 2 * c.border)
        {
            if i < n - 1 {
                if i % 2 == 1 {
                    nh = (nh - ih) / 2;
                } else {
                    nw = (nw - iv) / 2;
                }
                if (i % 4) == 2 && !dwindle {
                    nx += nw + iv;
                } else if (i % 4) == 3 && !dwindle {
                    ny += nh + ih;
                }
            }
            match i % 4 {
                0 => {
                    if dwindle {
                        ny += nh + ih;
                    } else {
                        ny -= nh + ih;
                    }
                }
                1 => nx += nw + iv,
                2 => ny += nh + ih,
                3 => {
                    if dwindle {
                        nx += nw + iv;
                    } else {
                        nx -= nw + iv;
                    }
                }
                _ => unreachable!(),
            }
            if i == 0 {
                if n != 1 {
                    nw = ((wa.w - 2 * ov - iv) as f32 * mfact) as i32;
                }
                ny = wa.y + oh;
            } else if i == 1 {
                nw = wa.w - nw - iv - 2 * ov;
            }
            i += 1;
        }
        out.push(Rect::new(nx, ny, nw - 2 * bw, nh - 2 * bw));
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn centered_master(
    tiled: &[&Client],
    wa: Rect,
    mfact: f32,
    nmaster: usize,
    oh: i32,
    ov: i32,
    ih: i32,
    iv: i32,
    bw: i32,
) -> Vec<Rect> {
    let n = tiled.len();
    let mut mx = wa.x + ov;
    let my = wa.y + oh;
    let mh = wa.h - 2 * oh - ih * ((if nmaster == 0 { n } else { n.min(nmaster) } as i32) - 1).max(0);
    let mut mw = wa.w - 2 * ov;
    let mut lx = 0;
    let ly = wa.y + oh;
    let mut lw = 0;
    let lh = wa.h - 2 * oh - ih * ((((n as i32 - nmaster as i32) / 2) - 1).max(0));
    let mut rx = 0;
    let ry = wa.y + oh;
    let mut rw = 0;
    let rh = wa.h
        - 2 * oh
        - ih * ((((n as i32 - nmaster as i32) / 2)
            - if (n as i32 - nmaster as i32) % 2 != 0 {
                0
            } else {
                1
            })
        .max(0));

    if nmaster > 0 && n > nmaster {
        if n - nmaster > 1 {
            mw = ((wa.w - 2 * ov - 2 * iv) as f32 * mfact) as i32;
            lw = (wa.w - mw - 2 * ov - 2 * iv) / 2;
            mx += lw + iv;
        } else {
            mw = ((mw - iv) as f32 * mfact) as i32;
            lw = wa.w - mw - iv - 2 * ov;
        }
        rw = lw;
        lx = wa.x + ov;
        rx = mx + mw + iv;
    }

    let mut mfacts = 0.0f32;
    let mut lfacts = 0.0f32;
    let mut rfacts = 0.0f32;
    for i in 0..n {
        if nmaster == 0 || i < nmaster {
            mfacts += 1.0;
        } else if (i - nmaster) % 2 == 1 {
            lfacts += 1.0;
        } else {
            rfacts += 1.0;
        }
    }

    let mut mtotal = 0;
    let mut ltotal = 0;
    let mut rtotal = 0;
    for i in 0..n {
        if nmaster == 0 || i < nmaster {
            if mfacts > 0.0 {
                mtotal += (mh as f32 / mfacts) as i32;
            }
        } else if (i - nmaster) % 2 == 1 {
            if lfacts > 0.0 {
                ltotal += (lh as f32 / lfacts) as i32;
            }
        } else if rfacts > 0.0 {
            rtotal += (rh as f32 / rfacts) as i32;
        }
    }
    let mrest = mh - mtotal;
    let lrest = lh - ltotal;
    let rrest = rh - rtotal;

    let mut out = Vec::with_capacity(n);
    let mut my_cur = my;
    let mut ly_cur = ly;
    let mut ry_cur = ry;

    for i in 0..n {
        if nmaster == 0 || i < nmaster {
            let h = if mfacts > 0.0 {
                (mh as f32 / mfacts) as i32 + if (i as i32) < mrest { 1 } else { 0 }
            } else {
                mh
            };
            out.push(Rect::new(mx, my_cur, mw - 2 * bw, h - 2 * bw));
            my_cur += h + ih;
        } else if (i - nmaster) % 2 == 1 {
            let h = if lfacts > 0.0 {
                (lh as f32 / lfacts) as i32
                    + if ((i as i32) - 2 * nmaster as i32) < 2 * lrest {
                        1
                    } else {
                        0
                    }
            } else {
                lh
            };
            out.push(Rect::new(lx, ly_cur, lw - 2 * bw, h - 2 * bw));
            ly_cur += h + ih;
        } else {
            let h = if rfacts > 0.0 {
                (rh as f32 / rfacts) as i32
                    + if ((i as i32) - 2 * nmaster as i32) < 2 * rrest {
                        1
                    } else {
                        0
                    }
            } else {
                rh
            };
            out.push(Rect::new(rx, ry_cur, rw - 2 * bw, h - 2 * bw));
            ry_cur += h + ih;
        }
    }
    let _ = (lx, ly, lw, lh, rx, ry, rw, rh, mx, my, mh, mw);
    out
}

#[allow(clippy::too_many_arguments)]
fn centered_floating_master(
    tiled: &[&Client],
    wa: Rect,
    mfact: f32,
    nmaster: usize,
    oh: i32,
    ov: i32,
    ih: i32,
    iv: i32,
    bw: i32,
) -> Vec<Rect> {
    let _ = ih;
    let n = tiled.len();
    let mut mx = wa.x + ov;
    let mut my = wa.y + oh;
    let mut sx = mx;
    let mut sy = my;
    let mut mh = wa.h - 2 * oh;
    let sh = mh;
    let mut mw = wa.w - 2 * ov - iv * ((n as i32) - 1).max(0);
    let sw = wa.w - 2 * ov - iv * ((n as i32 - nmaster as i32) - 1).max(0);
    let mut mivf = 1.0f32;

    if nmaster > 0 && n > nmaster {
        mivf = 0.8;
        if wa.w > wa.h {
            mw = (wa.w as f32 * mfact - iv as f32 * mivf * ((n.min(nmaster) as i32) - 1).max(0) as f32)
                as i32;
            mh = (wa.h as f32 * 0.9) as i32 - 2 * oh;
        } else {
            mw = (wa.w as f32 * 0.9 - iv as f32 * mivf * ((n.min(nmaster) as i32) - 1).max(0) as f32)
                as i32;
            mh = (wa.h as f32 * mfact) as i32;
        }
        mx = wa.x + (wa.w - mw) / 2;
        my = wa.y + (wa.h - mh) / 2;
        sx = wa.x + ov;
        sy = wa.y + oh;
    }

    let (mfacts, sfacts, mrest, srest) = get_facts(n, nmaster, mw, sw);
    let mut out = Vec::with_capacity(n);

    for (i, _) in tiled.iter().enumerate() {
        if i < nmaster {
            let w = if mfacts > 0.0 {
                (mw as f32 / mfacts) as i32 + if i < mrest as usize { 1 } else { 0 }
            } else {
                mw
            };
            out.push(Rect::new(mx, my, w - 2 * bw, mh - 2 * bw));
            mx += w + (iv as f32 * mivf) as i32;
        } else {
            let w = if sfacts > 0.0 {
                (sw as f32 / sfacts) as i32
                    + if (i - nmaster) < srest as usize {
                        1
                    } else {
                        0
                    }
            } else {
                sw
            };
            out.push(Rect::new(sx, sy, w - 2 * bw, sh - 2 * bw));
            sx += w + iv;
        }
    }
    out
}
