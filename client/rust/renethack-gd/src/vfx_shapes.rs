//! The drawn shapes of particles that are things, not light: a hot speck,
//! a sliver of ice, runes, a wisp of light rising. Stock star sprites (a
//! four-point sparkle on every cast, glint and shard) read as glitter; each
//! of these is what its effect is made of.

use super::*;

/// A hot point: a small bright heart in a faint round glow, no rays.
pub(super) fn speck() -> Gd<Texture2D> {
    texture(64, |x, y| {
        let d = ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt() * 2.0;
        let heart = (-(d / 0.2).powi(2)).exp();
        let glow = (1.0 - d).clamp(0.0, 1.0).powi(2) * 0.22;
        heart.max(glow)
    })
}

/// A sliver of ice: a long thin kite, its widest a third of the way
/// along, one side a little fuller than the other, its edges hard.
pub(super) fn sliver() -> Gd<Texture2D> {
    texture(64, |x, y| {
        let (px, py) = (x - 0.5, y - 0.5);
        let half = 0.44;
        if py.abs() >= half {
            return 0.0;
        }
        // from its blunt end (-half) to its point (half)
        let t = (py + half) / (2.0 * half);
        let grow = (t / 0.3).min(1.0);
        let taper = ((1.0 - t) / 0.7).min(1.0);
        let wide = 0.085 * grow.powf(0.7) * taper;
        let side = if px < 0.0 { 0.7 } else { 1.0 };
        let edge = 1.0 - px.abs() / (wide * side).max(1e-4);
        // a ridge of light down it, its faces dimmer
        let face = 0.55 + 0.45 * (1.0 - (px / 0.02).abs()).clamp(0.0, 1.0);
        (edge * 6.0).clamp(0.0, 1.0) * face
    })
}

/// A wisp of light rising: a small bright head, a thin tail fading below.
pub(super) fn wisp() -> Gd<Texture2D> {
    texture(64, |x, y| {
        let px = x - 0.5;
        let head = (-((y - 0.25) / 0.07).powi(2) - (px / 0.06).powi(2)).exp();
        let below = y - 0.25;
        let tail = if below > 0.0 {
            (-below / 0.22).exp() * (-(px / (0.018 + 0.03 * (1.0 - below))).powi(2)).exp() * 0.55
        } else {
            0.0
        };
        head.max(tail)
    })
}

/// Four runes, two by two (a particle shows one of them: `Recipe::picks`):
/// strokes cut straight and angular, in a faint glow.
pub(super) fn runes() -> Gd<Texture2D> {
    // each a few strokes in its square (0..1, y down)
    type Stroke = ((f32, f32), (f32, f32));
    const RUNES: [&[Stroke]; 4] = [
        // a stave, an arm up and an arm down from its middle
        &[
            ((0.42, 0.18), (0.42, 0.82)),
            ((0.42, 0.42), (0.66, 0.22)),
            ((0.42, 0.52), (0.66, 0.74)),
        ],
        // a peak over a bar
        &[
            ((0.3, 0.8), (0.5, 0.2)),
            ((0.5, 0.2), (0.7, 0.8)),
            ((0.34, 0.58), (0.66, 0.58)),
        ],
        // a stave crossed aslant, a tick at its foot
        &[
            ((0.5, 0.16), (0.5, 0.84)),
            ((0.32, 0.34), (0.68, 0.56)),
            ((0.5, 0.84), (0.64, 0.72)),
        ],
        // a lozenge on a stem
        &[
            ((0.5, 0.16), (0.68, 0.38)),
            ((0.68, 0.38), (0.5, 0.6)),
            ((0.5, 0.6), (0.32, 0.38)),
            ((0.32, 0.38), (0.5, 0.16)),
            ((0.5, 0.6), (0.5, 0.86)),
        ],
    ];
    texture(128, |x, y| {
        let (cx, cy) = ((x * 2.0).floor().min(1.0), (y * 2.0).floor().min(1.0));
        let (u, v) = (x * 2.0 - cx, y * 2.0 - cy);
        let rune = RUNES[(cy as usize) * 2 + cx as usize];
        let mut near = f32::MAX;
        for &((ax, ay), (bx, by)) in rune {
            let (abx, aby) = (bx - ax, by - ay);
            let t = (((u - ax) * abx + (v - ay) * aby) / (abx * abx + aby * aby)).clamp(0.0, 1.0);
            let (dx, dy) = (u - ax - abx * t, v - ay - aby * t);
            near = near.min((dx * dx + dy * dy).sqrt());
        }
        let cut = (1.0 - (near - 0.022) / 0.02).clamp(0.0, 1.0);
        let glow = (-(near / 0.09).powi(2)).exp() * 0.25;
        // (nothing of it over its square's edge into the next rune)
        let inside = ((u.min(1.0 - u).min(v).min(1.0 - v)) / 0.06).clamp(0.0, 1.0);
        cut.max(glow) * inside
    })
}
