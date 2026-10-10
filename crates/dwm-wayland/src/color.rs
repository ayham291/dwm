//! Color helpers for dwm-style borders / chrome.

use smithay::backend::renderer::Color32F;

/// Parse `#RRGGBB` or `#RRGGBBAA` into a GPU color. Falls back to opaque gray.
pub fn parse_hex(hex: &str) -> Color32F {
    let s = hex.trim().trim_start_matches('#');
    let mut chars = s.chars();
    let mut nibble = || -> Option<u8> {
        let a = chars.next()?.to_digit(16)? as u8;
        let b = chars.next()?.to_digit(16)? as u8;
        Some((a << 4) | b)
    };
    let r = nibble().unwrap_or(0x44) as f32 / 255.0;
    let g = nibble().unwrap_or(0x44) as f32 / 255.0;
    let b = nibble().unwrap_or(0x44) as f32 / 255.0;
    let a = nibble().unwrap_or(0xff) as f32 / 255.0;
    Color32F::new(r, g, b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sel_border_gold() {
        let c = parse_hex("#E0AA00");
        assert!((c.r() - 0xE0 as f32 / 255.0).abs() < 0.001);
        assert!((c.g() - 0xAA as f32 / 255.0).abs() < 0.001);
        assert!((c.b() - 0.0).abs() < 0.001);
        assert!((c.a() - 1.0).abs() < 0.001);
    }
}
