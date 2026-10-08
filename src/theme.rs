//! Colors for the keycap look, independent of how a platform draws them.

/// sRGB color, components in `0..=1`.
pub type Rgba = [f64; 4];

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub tray: Rgba,
    pub tray_border: Rgba,
    pub shadow: Rgba,
    pub keys: KeyColors,
    pub accent_keys: KeyColors,
    /// Typed text in typing mode.
    pub text: Rgba,
    /// Spaces in typed text, drawn as `␣`.
    pub text_dim: Rgba,
    pub badge: Rgba,
    pub badge_text: Rgba,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyColors {
    /// The sides of the keycap, seen below its top face.
    pub skirt: Rgba,
    pub face: Rgba,
    pub face_pressed: Rgba,
    /// Hairline bevel around the top face.
    pub bevel: Rgba,
    /// Light falling on the top of the face.
    pub sheen: Rgba,
    pub legend: Rgba,
    pub name: Rgba,
}

impl Palette {
    pub fn new(dark: bool, accent: Rgba) -> Palette {
        let accent_keys = KeyColors {
            skirt: shade(accent, 0.62),
            face: accent,
            face_pressed: shade(accent, 0.88),
            bevel: [1.0, 1.0, 1.0, 0.28],
            sheen: [1.0, 1.0, 1.0, 0.22],
            legend: [1.0, 1.0, 1.0, 1.0],
            name: [1.0, 1.0, 1.0, 0.82],
        };
        let badge_text = if luminance(accent) > 0.6 {
            [0.08, 0.08, 0.09, 1.0]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };
        if dark {
            Palette {
                tray: [0.06, 0.06, 0.07, 0.72],
                tray_border: [1.0, 1.0, 1.0, 0.09],
                shadow: [0.0, 0.0, 0.0, 0.55],
                keys: KeyColors {
                    skirt: [0.07, 0.07, 0.08, 1.0],
                    face: [0.20, 0.20, 0.22, 1.0],
                    face_pressed: [0.16, 0.16, 0.18, 1.0],
                    bevel: [1.0, 1.0, 1.0, 0.10],
                    sheen: [1.0, 1.0, 1.0, 0.07],
                    legend: [0.96, 0.96, 0.97, 1.0],
                    name: [0.64, 0.64, 0.68, 1.0],
                },
                accent_keys,
                text: [0.97, 0.97, 0.98, 1.0],
                text_dim: [1.0, 1.0, 1.0, 0.30],
                badge: accent,
                badge_text,
            }
        } else {
            Palette {
                tray: [0.97, 0.97, 0.98, 0.78],
                tray_border: [0.0, 0.0, 0.0, 0.08],
                shadow: [0.0, 0.0, 0.0, 0.28],
                keys: KeyColors {
                    skirt: [0.73, 0.73, 0.74, 1.0],
                    face: [1.0, 1.0, 1.0, 1.0],
                    face_pressed: [0.93, 0.93, 0.95, 1.0],
                    bevel: [0.0, 0.0, 0.0, 0.07],
                    sheen: [1.0, 1.0, 1.0, 0.0],
                    legend: [0.11, 0.11, 0.12, 1.0],
                    name: [0.45, 0.45, 0.48, 1.0],
                },
                accent_keys,
                text: [0.11, 0.11, 0.12, 1.0],
                text_dim: [0.0, 0.0, 0.0, 0.28],
                badge: accent,
                badge_text,
            }
        }
    }
}

fn shade([r, g, b, a]: Rgba, k: f64) -> Rgba {
    [r * k, g * k, b * k, a]
}

fn luminance([r, g, b, _]: Rgba) -> f64 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_text_contrasts_with_accent() {
        let light_accent = Palette::new(true, [1.0, 0.9, 0.3, 1.0]);
        assert!(luminance(light_accent.badge_text) < 0.2);
        let dark_accent = Palette::new(true, [0.2, 0.3, 0.8, 1.0]);
        assert!(luminance(dark_accent.badge_text) > 0.9);
    }
}
