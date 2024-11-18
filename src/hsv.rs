#[derive(Debug, Clone)]
pub struct HSVColor {
    pub hue: f32,
    pub sat: f32,
    pub val: f32,
}

impl HSVColor {
    pub fn from_rgb<T: Into<f32>>(red: T, green: T, blue: T, depth: u8) -> Self {
        let n: f32 = ((1 << depth) - 1) as f32;
        let (r, g, b) = (red.into() / n, green.into() / n, blue.into() / n);

        let v: f32 = r.max(g.max(b));
        let c: f32 = v - r.min(g.min(b));
        
        let h: f32 =
            (if c == 0.0 {
                0.0
            } else if v == r {
                ((g - b) / c) % 6.0
            } else if v == g {
                ((b - r) / c) + 2.0
            } else if v == b {
                ((r - g) / c) + 4.0
            } else {
                unreachable!();
            }) * 60.0;

        let s: f32 =
            if v == 0.0 { 0.0 } else { c / v };

        HSVColor { hue: h, sat: s, val: v }
    }

    pub fn to_rgb(&self, depth: u8) -> [u8; 3] {
        let c = self.val * self.sat;
        let h = self.hue / 60.0;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let m = self.val - c;

        let (r1, g1, b1): (f32, f32, f32) = match h {
            v if v < 1.0 => (c, x, 0.0),
            v if v < 2.0 => (x, c, 0.0),
            v if v < 3.0 => (0.0, c, x),
            v if v < 4.0 => (0.0, x, c),
            v if v < 5.0 => (x, 0.0, c),
            v if v < 6.0 => (c, 0.0, x),
            _ => unreachable!()
        };

        let n = ((1 << depth) - 1) as f32;
        let r = ((r1 + m) * n) as u8;
        let g = ((g1 + m) * n) as u8;
        let b = ((b1 + m) * n) as u8;

        [r, g, b]
    }
}
