use bevy::prelude::*;
use rand::{rngs::StdRng, Rng};

pub fn hex_srgb_u8(hex: &str) -> Color {
    let h = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(255);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(255);
    Color::srgb_u8(r, g, b)
}

pub fn weighted_pick(rng: &mut StdRng, weights: &[u32]) -> usize {
    let tot: u32 = weights.iter().copied().sum();
    let mut roll = rng.gen_range(0..tot);
    for (i, &w) in weights.iter().enumerate() {
        if roll < w { return i; }
        roll -= w;
    }
    weights.len() - 1
}