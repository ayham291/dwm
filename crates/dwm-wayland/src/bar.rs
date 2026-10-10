//! Classic dwm status bar: tags · layout · title · status.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use dwm_core::BarModel;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::Color32F;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::Transform;
use tracing::warn;

use crate::color;

const FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
];

pub struct StatusBar {
    font_data: Vec<u8>,
    buffer: Option<MemoryRenderBuffer>,
    last_key: String,
}

impl StatusBar {
    pub fn new() -> Self {
        let font_data = FONT_CANDIDATES
            .iter()
            .find_map(|path| std::fs::read(path).ok())
            .unwrap_or_else(|| {
                warn!("no system monospace font found; bar text may be empty");
                Vec::new()
            });
        Self {
            font_data,
            buffer: None,
            last_key: String::new(),
        }
    }

    pub fn element(
        &mut self,
        renderer: &mut GlesRenderer,
        model: &BarModel,
        screen_h: i32,
        cfg_norm_bg: &str,
        cfg_norm_fg: &str,
        cfg_sel_bg: &str,
        cfg_sel_fg: &str,
    ) -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
        if model.height <= 0 || model.width <= 0 {
            return None;
        }

        let key = format!(
            "{}|{}|{}|{}|{}|{}",
            model.width,
            model.height,
            model.layout_symbol,
            model.title,
            model.status,
            model
                .tags
                .iter()
                .map(|t| format!("{}:{}:{}", t.label, t.selected as u8, t.occupied as u8))
                .collect::<Vec<_>>()
                .join(",")
        );

        if self.last_key != key || self.buffer.is_none() {
            self.buffer = Some(paint(
                &self.font_data,
                model,
                cfg_norm_bg,
                cfg_norm_fg,
                cfg_sel_bg,
                cfg_sel_fg,
            ));
            self.last_key = key;
        }

        let buffer = self.buffer.as_ref()?;
        let y = if model.top {
            0.0
        } else {
            (screen_h - model.height) as f64
        };
        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            (0.0, y),
            buffer,
            None,
            None,
            None,
            Kind::Unspecified,
        )
        .ok()
    }
}

fn paint(
    font_data: &[u8],
    model: &BarModel,
    cfg_norm_bg: &str,
    cfg_norm_fg: &str,
    cfg_sel_bg: &str,
    cfg_sel_fg: &str,
) -> MemoryRenderBuffer {
    let w = model.width.max(1) as usize;
    let h = model.height.max(1) as usize;
    let mut pixels = vec![0u8; w * h * 4]; // RGBA

    let norm_bg = color::parse_hex(cfg_norm_bg);
    let norm_fg = color::parse_hex(cfg_norm_fg);
    let sel_bg = color::parse_hex(cfg_sel_bg);
    let sel_fg = color::parse_hex(cfg_sel_fg);

    fill_rect(&mut pixels, w, h, 0, 0, w as i32, h as i32, norm_bg);

    let font = if font_data.is_empty() {
        None
    } else {
        FontRef::try_from_slice(font_data).ok()
    };
    let scale = PxScale::from((h as f32 * 0.7).max(10.0));
    let pad = 8i32;

    let mut x = 0i32;
    for tag in &model.tags {
        let (bg, fg) = if tag.selected {
            (sel_bg, sel_fg)
        } else {
            (norm_bg, norm_fg)
        };
        let label = format!(" {} ", tag.label);
        let tw = text_width(font.as_ref(), scale, &label).max(pad);
        fill_rect(&mut pixels, w, h, x, 0, tw, h as i32, bg);
        if let Some(font) = font.as_ref() {
            draw_text(
                &mut pixels,
                w,
                h,
                x + 2,
                (h as i32 - scale.y as i32) / 2,
                font,
                scale,
                &label,
                fg,
            );
        }
        x += tw;
    }

    let lt = format!(" {} ", model.layout_symbol);
    let ltw = text_width(font.as_ref(), scale, &lt).max(pad);
    fill_rect(&mut pixels, w, h, x, 0, ltw, h as i32, norm_bg);
    if let Some(font) = font.as_ref() {
        draw_text(
            &mut pixels,
            w,
            h,
            x + 2,
            (h as i32 - scale.y as i32) / 2,
            font,
            scale,
            &lt,
            norm_fg,
        );
    }
    x += ltw;

    let status = format!(" {} ", model.status);
    let sw = text_width(font.as_ref(), scale, &status);
    let sx = (w as i32 - sw).max(x);
    if let Some(font) = font.as_ref() {
        draw_text(
            &mut pixels,
            w,
            h,
            sx,
            (h as i32 - scale.y as i32) / 2,
            font,
            scale,
            &status,
            norm_fg,
        );
    }

    if sx > x + pad {
        let title = truncate_to_width(font.as_ref(), scale, &model.title, sx - x - pad);
        fill_rect(&mut pixels, w, h, x, 0, sx - x, h as i32, sel_bg);
        if let Some(font) = font.as_ref() {
            draw_text(
                &mut pixels,
                w,
                h,
                x + 4,
                (h as i32 - scale.y as i32) / 2,
                font,
                scale,
                &title,
                sel_fg,
            );
        }
    }

    // Pack RGBA -> ABGR8888 byte order for smithay.
    let mut abgr = vec![0u8; pixels.len()];
    for (dst, src) in abgr.chunks_exact_mut(4).zip(pixels.chunks_exact(4)) {
        dst[0] = src[2];
        dst[1] = src[1];
        dst[2] = src[0];
        dst[3] = src[3];
    }

    MemoryRenderBuffer::from_slice(
        &abgr,
        Fourcc::Abgr8888,
        (w as i32, h as i32),
        1,
        Transform::Normal,
        None,
    )
}

fn fill_rect(
    pixels: &mut [u8],
    stride_w: usize,
    h: usize,
    x: i32,
    y: i32,
    w: i32,
    hh: i32,
    color: Color32F,
) {
    let r = (color.r() * 255.0) as u8;
    let g = (color.g() * 255.0) as u8;
    let b = (color.b() * 255.0) as u8;
    let a = (color.a() * 255.0) as u8;
    for row in y.max(0)..(y + hh).min(h as i32) {
        for col in x.max(0)..(x + w).min(stride_w as i32) {
            let i = ((row as usize) * stride_w + col as usize) * 4;
            pixels[i] = r;
            pixels[i + 1] = g;
            pixels[i + 2] = b;
            pixels[i + 3] = a;
        }
    }
}

fn text_width(font: Option<&FontRef<'_>>, scale: PxScale, text: &str) -> i32 {
    let Some(font) = font else {
        return (text.len() as i32) * 8;
    };
    let scaled = font.as_scaled(scale);
    let mut w = 0.0f32;
    for ch in text.chars() {
        let glyph = scaled.scaled_glyph(ch);
        w += scaled.h_advance(glyph.id);
    }
    w.ceil() as i32
}

fn truncate_to_width(font: Option<&FontRef<'_>>, scale: PxScale, text: &str, max_w: i32) -> String {
    if max_w <= 0 {
        return String::new();
    }
    if text_width(font, scale, text) <= max_w {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars() {
        let trial = format!("{out}{ch}…");
        if text_width(font, scale, &trial) > max_w {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

fn draw_text(
    pixels: &mut [u8],
    stride_w: usize,
    h: usize,
    x: i32,
    y: i32,
    font: &FontRef<'_>,
    scale: PxScale,
    text: &str,
    color: Color32F,
) {
    let scaled = font.as_scaled(scale);
    let mut caret = x as f32;
    let baseline = y as f32 + scaled.ascent();
    let cr = color.r();
    let cg = color.g();
    let cb = color.b();
    let ca = color.a();

    for ch in text.chars() {
        let mut glyph = scaled.scaled_glyph(ch);
        glyph.position = point(caret, baseline);
        caret += scaled.h_advance(glyph.id);
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, cover| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px < 0 || py < 0 || px >= stride_w as i32 || py >= h as i32 {
                    return;
                }
                let i = (py as usize * stride_w + px as usize) * 4;
                let cover = cover.clamp(0.0, 1.0) * ca;
                let inv = 1.0 - cover;
                pixels[i] = (((pixels[i] as f32 / 255.0) * inv + cr * cover) * 255.0) as u8;
                pixels[i + 1] =
                    (((pixels[i + 1] as f32 / 255.0) * inv + cg * cover) * 255.0) as u8;
                pixels[i + 2] =
                    (((pixels[i + 2] as f32 / 255.0) * inv + cb * cover) * 255.0) as u8;
                pixels[i + 3] = 255;
            });
        }
    }
}
