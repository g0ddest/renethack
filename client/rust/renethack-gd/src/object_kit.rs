//! The procedural objects of the art kit: rings, amulets, wands, gems,
//! potions, armour, bows... built from primitive meshes so that each reads
//! as itself on the map and as an icon (the icon bake draws them). The
//! variant (`shape`) comes from the manifest rule the appearance matched;
//! the colour is the look's tint, which the appearance gives.

use super::*;

const ONE: [f32; 3] = [1.0; 3];
const FLAT: [f32; 3] = [0.0; 3];

/// A colour made brighter and more saturated (a stone, a liquid).
fn vivid(c: Color, gain: f32) -> Color {
    let grey = (c.r + c.g + c.b) / 3.0;
    let s = |v: f32| ((grey + (v - grey) * 1.35) * gain).clamp(0.0, 1.0);
    Color::from_rgba(s(c.r), s(c.g), s(c.b), 1.0)
}

impl Kit<'_> {
    fn mat(&mut self, c: Color, finish: Finish) -> Gd<Material> {
        self.art.flat(c, finish)
    }

    fn shape_is(&self, s: &str) -> bool {
        self.shape.as_deref() == Some(s)
    }

    /// A manifest material, untinted (a gold band under a coloured stone).
    fn plain(&mut self, name: &str, fallback: Color) -> Gd<Material> {
        self.named(name, fallback)
    }

    fn leather(&mut self) -> Gd<Material> {
        self.plain("leather", Color::from_rgb(0.35, 0.22, 0.12))
    }

    /// The look's colour as a polished stone or liquid.
    fn gem_mat(&mut self) -> Gd<Material> {
        let c = vivid(self.tint, 1.0);
        self.mat(c, Finish::Gem)
    }

    /// A brilliant-cut stone `r` across, its point at `at` (y up).
    fn cut_stone(&mut self, parent: &mut Gd<Node3D>, mat: &Gd<Material>, at: [f32; 3], r: f32) {
        let [x, y, z] = at;
        self.part(
            parent,
            cylinder(r, 0.0, r * 0.9),
            mat,
            [x, y + r * 0.45, z],
            FLAT,
            ONE,
        );
        self.part(
            parent,
            cylinder(r * 0.62, r, r * 0.4),
            mat,
            [x, y + r * 1.1, z],
            FLAT,
            ONE,
        );
    }

    /// Objects: each one unit across (its length or height), resting on y = 0.
    pub(super) fn object(&mut self, kind: Proc, root: &mut Gd<Node3D>) {
        match kind {
            Proc::Ring => self.ring(root),
            Proc::Amulet => self.amulet(root),
            Proc::Wand => self.wand(root),
            Proc::Gem => self.gem(root),
            Proc::Potion => self.potion(root),
            Proc::Helm => self.helm(root),
            Proc::Boots => self.boots(root),
            Proc::Gloves => self.gloves(root),
            Proc::Garment => self.garment(root),
            Proc::Cuirass => self.cuirass(root),
            Proc::Bow => self.bow(root),
            Proc::Horn => self.horn(root),
            _ => self.simple(kind, root),
        }
    }

    /// A ring lying flat: a band of the appearance's material, or a gold
    /// band set with a stone of its colour.
    fn ring(&mut self, root: &mut Gd<Node3D>) {
        let gem = self.shape_is("gem");
        let band = if gem {
            self.plain("gold", Color::from_rgb(0.8, 0.6, 0.2))
        } else {
            self.skin.clone()
        };
        if self.shape_is("twisted") {
            for (y, a) in [(0.05f32, 8.0f32), (0.1, -8.0)] {
                self.part(
                    root,
                    torus(0.38, 0.46),
                    &band,
                    [0.0, y, 0.0],
                    [a, 0.0, a],
                    ONE,
                );
            }
            return;
        }
        if self.shape_is("wire") {
            for y in [0.03f32, 0.08, 0.13] {
                self.part(
                    root,
                    torus(0.4, 0.45),
                    &band,
                    [0.0, y, 0.0],
                    [4.0, 0.0, 0.0],
                    ONE,
                );
            }
            return;
        }
        self.part(
            root,
            torus(0.34, 0.5),
            &band,
            [0.0, 0.08, 0.0],
            FLAT,
            [1.0, 1.15, 1.0],
        );
        if gem {
            let stone = self.gem_mat();
            self.part(
                root,
                cylinder(0.15, 0.19, 0.12),
                &band,
                [0.0, 0.16, 0.42],
                [-15.0, 0.0, 0.0],
                ONE,
            );
            self.cut_stone(root, &stone, [0.0, 0.1, 0.44], 0.22);
        } else {
            // a raised middle line on the band
            let dark = self.dark();
            self.part(
                root,
                torus(0.495, 0.51),
                &dark,
                [0.0, 0.08, 0.0],
                FLAT,
                [1.0, 0.3, 1.0],
            );
        }
    }

    /// An amulet: a chain and a pendant of its shape, all of one metal.
    fn amulet(&mut self, root: &mut Gd<Node3D>) {
        let metal = self.skin.clone();
        self.part(
            root,
            torus(0.3, 0.325),
            &metal,
            [0.0, 0.015, -0.2],
            FLAT,
            [1.0, 1.0, 0.8],
        );
        self.part(
            root,
            torus(0.04, 0.07),
            &metal,
            [0.0, 0.06, 0.12],
            [90.0, 0.0, 0.0],
            ONE,
        );
        let at = [0.0, 0.0, 0.34];
        let mut pendant = self.pivot(root, "Pendant", [0.0, 0.0, 0.0]);
        pendant.set_scale(Vector3::new(1.35, 1.35, 1.35));
        pendant.set_position(Vector3::new(0.0, 0.0, -0.12));
        let root = &mut pendant;
        let dark = self.dark();
        let shape = self.shape.clone().unwrap_or_default();
        let (x, z) = (at[0], at[2]);
        match shape.as_str() {
            "spherical" => self.part(root, sphere(0.2), &metal, [x, 0.2, z], FLAT, ONE),
            "oval" => {
                self.part(
                    root,
                    cylinder(0.2, 0.2, 0.06),
                    &metal,
                    [x, 0.03, z],
                    FLAT,
                    [0.75, 1.0, 1.3],
                );
                self.part(
                    root,
                    dome(0.12),
                    &metal,
                    [x, 0.06, z],
                    FLAT,
                    [0.75, 1.0, 1.3],
                );
            }
            "triangular" => self.part(
                root,
                prism(0.5, 0.45, 0.07),
                &metal,
                [x, 0.035, z],
                [-90.0, 180.0, 0.0],
                ONE,
            ),
            "pyramidal" => self.part(
                root,
                facets(0.24, 4),
                &metal,
                [x, 0.16, z],
                [0.0, 45.0, 0.0],
                [1.0, 0.9, 1.0],
            ),
            "square" => {
                self.part(
                    root,
                    cuboid(0.36, 0.06, 0.36),
                    &metal,
                    [x, 0.03, z],
                    [0.0, 45.0, 0.0],
                    ONE,
                );
                self.part(
                    root,
                    cuboid(0.2, 0.05, 0.2),
                    &metal,
                    [x, 0.08, z],
                    [0.0, 45.0, 0.0],
                    ONE,
                );
            }
            "concave" => {
                self.part(
                    root,
                    dome(0.26),
                    &metal,
                    [x, 0.13, z],
                    [180.0, 0.0, 0.0],
                    [1.0, 0.8, 1.0],
                );
                self.part(
                    root,
                    sphere(0.2),
                    &dark,
                    [x, 0.16, z],
                    FLAT,
                    [1.0, 0.25, 1.0],
                );
            }
            "hexagonal" => self.part(
                root,
                facets(0.27, 6),
                &metal,
                [x, 0.05, z],
                FLAT,
                [1.0, 0.2, 1.0],
            ),
            "octagonal" => self.part(
                root,
                facets(0.27, 8),
                &metal,
                [x, 0.05, z],
                FLAT,
                [1.0, 0.2, 1.0],
            ),
            "cubical" => self.part(
                root,
                cuboid(0.28, 0.28, 0.28),
                &metal,
                [x, 0.14, z],
                [0.0, 30.0, 0.0],
                ONE,
            ),
            "perforated" => {
                self.part(
                    root,
                    cylinder(0.26, 0.26, 0.06),
                    &metal,
                    [x, 0.03, z],
                    FLAT,
                    ONE,
                );
                for i in 0..6 {
                    let a = i as f32 * std::f32::consts::TAU / 6.0;
                    self.part(
                        root,
                        cylinder(0.045, 0.045, 0.02),
                        &dark,
                        [x + 0.15 * a.cos(), 0.061, z + 0.15 * a.sin()],
                        FLAT,
                        ONE,
                    );
                }
                self.part(
                    root,
                    cylinder(0.05, 0.05, 0.02),
                    &dark,
                    [x, 0.061, z],
                    FLAT,
                    ONE,
                );
            }
            "yendor" => {
                self.part(
                    root,
                    cylinder(0.27, 0.27, 0.06),
                    &metal,
                    [x, 0.03, z],
                    FLAT,
                    ONE,
                );
                self.part(
                    root,
                    torus(0.22, 0.28),
                    &metal,
                    [x, 0.07, z],
                    FLAT,
                    [1.0, 0.6, 1.0],
                );
                let ruby = self.mat(Color::from_rgb(0.9, 0.08, 0.1), Finish::Gem);
                self.cut_stone(root, &ruby, [x, 0.04, z], 0.14);
            }
            // circular
            _ => {
                self.part(
                    root,
                    cylinder(0.26, 0.26, 0.06),
                    &metal,
                    [x, 0.03, z],
                    FLAT,
                    ONE,
                );
                self.part(
                    root,
                    torus(0.2, 0.27),
                    &metal,
                    [x, 0.07, z],
                    FLAT,
                    [1.0, 0.6, 1.0],
                );
                self.part(root, dome(0.1), &metal, [x, 0.06, z], FLAT, ONE);
            }
        }
    }

    /// A wand lying along z: a tapered rod, a grip, a crystal at the tip,
    /// and what its appearance names (runes, a fork, spikes, jewels).
    fn wand(&mut self, root: &mut Gd<Node3D>) {
        let long = self.shape_is("long");
        let short = self.shape_is("short");
        let len = if long {
            1.0
        } else if short {
            0.7
        } else {
            0.9
        };
        let r = if short { 0.045 } else { 0.035 };
        let skin = self.skin.clone();
        let h = len * 0.5;
        if self.shape_is("curved") {
            for (i, a) in [(0, 0.0f32), (1, 8.0), (2, 18.0)] {
                let z = -h + (i as f32 + 0.5) * len / 3.0;
                let x = [0.0, 0.015, 0.06][i];
                self.part(
                    root,
                    cylinder(r * 0.9, r, len / 3.0 + 0.02),
                    &skin,
                    [x, r, z],
                    [90.0, a, 0.0],
                    ONE,
                );
            }
        } else if self.shape_is("hexagonal") {
            self.part(
                root,
                facets(r * 1.3, 6),
                &skin,
                [0.0, r, 0.0],
                [90.0, 0.0, 0.0],
                [1.0, len / (r * 2.6), 1.0],
            );
        } else {
            self.part(
                root,
                cylinder(r * 0.75, r * 1.1, len),
                &skin,
                [0.0, r, 0.0],
                [90.0, 0.0, 0.0],
                ONE,
            );
        }
        // grip and pommel
        let dark = self.dark();
        let gold = self.plain("gilded", Color::from_rgb(0.7, 0.55, 0.2));
        self.part(
            root,
            cylinder(r * 1.35, r * 1.35, 0.2),
            &dark,
            [0.0, r, -h + 0.14],
            [90.0, 0.0, 0.0],
            ONE,
        );
        for z in [-h + 0.03, -h + 0.25] {
            self.part(
                root,
                cylinder(r * 1.5, r * 1.5, 0.025),
                &gold,
                [0.0, r, z],
                [90.0, 0.0, 0.0],
                ONE,
            );
        }
        self.part(root, sphere(r * 1.4), &gold, [0.0, r, -h], FLAT, ONE);
        let tip = self.mat(Color::from_rgb(1.0, 0.86, 0.55), Finish::Gem);
        let end = [0.0, r, h + 0.02];
        if self.shape_is("forked") {
            for s in [-1.0f32, 1.0] {
                self.part(
                    root,
                    cylinder(r * 0.4, r * 0.7, 0.18),
                    &skin,
                    [s * 0.04, r, h + 0.06],
                    [90.0, s * 18.0, 0.0],
                    ONE,
                );
            }
            return;
        }
        if self.shape_is("spiked") {
            for i in 0..4 {
                let a = i as f32 * 90.0 + 45.0;
                let (sx, sy) = (a.to_radians().cos(), a.to_radians().sin());
                self.part(
                    root,
                    cylinder(0.0, r * 0.6, 0.09),
                    &gold,
                    [sx * r * 1.6, r + sy * r * 1.6, h - 0.08],
                    [0.0, 0.0, a - 90.0],
                    ONE,
                );
            }
        }
        if self.shape_is("runed") {
            for i in 0..4 {
                let z = -h + 0.35 + i as f32 * 0.12;
                self.part(
                    root,
                    cylinder(r * 1.05, r * 1.05, 0.02),
                    &tip,
                    [0.0, r, z],
                    [90.0, 0.0, 0.0],
                    ONE,
                );
            }
        }
        let jewel = self.shape_is("jeweled");
        let size = if jewel { r * 2.2 } else { r * 1.5 };
        let stone = if jewel { self.gem_mat() } else { tip };
        self.part(
            root,
            facets(size, 6),
            &stone,
            end,
            [90.0, 0.0, 0.0],
            [1.0, 1.5, 1.0],
        );
        if jewel {
            for z in [-h + 0.35, -h + 0.5] {
                self.part(
                    root,
                    facets(r * 0.9, 6),
                    &stone,
                    [0.0, r * 1.9, z],
                    FLAT,
                    ONE,
                );
            }
        }
    }

    /// A cut stone standing on its point, the table up.
    fn gem(&mut self, root: &mut Gd<Node3D>) {
        let stone = self.gem_mat();
        self.part(
            root,
            cylinder(0.5, 0.0, 0.5),
            &stone,
            [0.0, 0.25, 0.0],
            [0.0, 11.0, 0.0],
            ONE,
        );
        self.part(
            root,
            cylinder(0.3, 0.5, 0.22),
            &stone,
            [0.0, 0.61, 0.0],
            [0.0, 11.0, 0.0],
            ONE,
        );
        // a glint on the table
        let glint = self.mat(Color::from_rgb(1.0, 1.0, 1.0), Finish::Gem);
        self.part(root, facets(0.05, 4), &glint, [-0.12, 0.73, 0.1], FLAT, ONE);
    }

    /// A glass bottle of the look's liquid, stoppered; `shape` picks the
    /// bottle.
    fn potion(&mut self, root: &mut Gd<Node3D>) {
        let glass = self.mat(Color::from_rgba(0.82, 0.9, 0.95, 0.28), Finish::Glass);
        let liquid = self.gem_mat();
        let cork = self.plain("wood", Color::from_rgb(0.45, 0.3, 0.16));
        let wax = self.mat(Color::from_rgb(0.5, 0.08, 0.06), Finish::Glossy);
        let shape = self.shape.clone().unwrap_or_default();
        let (neck_y, neck_r) = match shape.as_str() {
            "bottle" => {
                self.part(
                    root,
                    cylinder(0.2, 0.2, 0.55),
                    &glass,
                    [0.0, 0.3, 0.0],
                    FLAT,
                    ONE,
                );
                self.part(
                    root,
                    sphere(0.2),
                    &glass,
                    [0.0, 0.575, 0.0],
                    FLAT,
                    [1.0, 0.6, 1.0],
                );
                self.part(
                    root,
                    cylinder(0.175, 0.175, 0.44),
                    &liquid,
                    [0.0, 0.25, 0.0],
                    FLAT,
                    ONE,
                );
                let paper = self.plain("paper", Color::from_rgb(0.8, 0.72, 0.55));
                self.part(
                    root,
                    cylinder(0.205, 0.205, 0.16),
                    &paper,
                    [0.0, 0.32, 0.0],
                    FLAT,
                    ONE,
                );
                (0.74, 0.07)
            }
            "vial" => {
                self.part(
                    root,
                    cylinder(0.13, 0.13, 0.62),
                    &glass,
                    [0.0, 0.43, 0.0],
                    FLAT,
                    ONE,
                );
                self.part(root, sphere(0.13), &glass, [0.0, 0.13, 0.0], FLAT, ONE);
                self.part(
                    root,
                    cylinder(0.11, 0.11, 0.45),
                    &liquid,
                    [0.0, 0.34, 0.0],
                    FLAT,
                    ONE,
                );
                self.part(root, sphere(0.11), &liquid, [0.0, 0.13, 0.0], FLAT, ONE);
                (0.78, 0.1)
            }
            "jug" => {
                self.part(
                    root,
                    sphere(0.38),
                    &glass,
                    [0.0, 0.3, 0.0],
                    FLAT,
                    [1.0, 0.78, 1.0],
                );
                self.part(
                    root,
                    sphere(0.34),
                    &liquid,
                    [0.0, 0.28, 0.0],
                    FLAT,
                    [1.0, 0.7, 1.0],
                );
                self.part(
                    root,
                    torus(0.1, 0.14),
                    &glass,
                    [0.33, 0.42, 0.0],
                    [0.0, 0.0, 90.0],
                    [1.0, 1.0, 1.0],
                );
                (0.6, 0.1)
            }
            // a round flask
            _ => {
                self.part(root, sphere(0.34), &glass, [0.0, 0.34, 0.0], FLAT, ONE);
                self.part(
                    root,
                    sphere(0.3),
                    &liquid,
                    [0.0, 0.32, 0.0],
                    FLAT,
                    [1.0, 0.85, 1.0],
                );
                (0.72, 0.08)
            }
        };
        self.part(
            root,
            cylinder(neck_r, neck_r * 1.2, 0.2),
            &glass,
            [0.0, neck_y, 0.0],
            FLAT,
            ONE,
        );
        self.part(
            root,
            torus(neck_r * 0.9, neck_r * 1.35),
            &glass,
            [0.0, neck_y + 0.1, 0.0],
            FLAT,
            [1.0, 1.5, 1.0],
        );
        self.part(
            root,
            cylinder(neck_r * 1.05, neck_r * 0.9, 0.12),
            &cork,
            [0.0, neck_y + 0.14, 0.0],
            FLAT,
            ONE,
        );
        self.part(
            root,
            dome(neck_r * 1.1),
            &wax,
            [0.0, neck_y + 0.2, 0.0],
            FLAT,
            ONE,
        );
    }

    /// Headgear by its appearance: a helmet (plumed, crested, visored), a
    /// hat with a brim, a cone, a fedora, a pot.
    fn helm(&mut self, root: &mut Gd<Node3D>) {
        let skin = self.skin.clone();
        let dark = self.dark();
        let shape = self.shape.clone().unwrap_or_default();
        match shape.as_str() {
            "hat" | "hardhat" => {
                let brim = if shape == "hat" { 0.5 } else { 0.4 };
                self.part(
                    root,
                    dome(0.3),
                    &skin,
                    [0.0, 0.05, 0.0],
                    FLAT,
                    [1.0, 1.3, 1.1],
                );
                self.part(
                    root,
                    cylinder(brim, brim, 0.04),
                    &skin,
                    [0.0, 0.05, 0.0],
                    FLAT,
                    [1.0, 1.0, 1.1],
                );
                self.part(
                    root,
                    cylinder(0.305, 0.305, 0.07),
                    &dark,
                    [0.0, 0.1, 0.0],
                    FLAT,
                    [1.0, 1.0, 1.1],
                );
            }
            "cone" => {
                self.part(
                    root,
                    cylinder(0.0, 0.3, 0.85),
                    &skin,
                    [0.0, 0.45, 0.0],
                    [6.0, 0.0, 0.0],
                    ONE,
                );
                self.part(
                    root,
                    cylinder(0.45, 0.45, 0.03),
                    &skin,
                    [0.0, 0.03, 0.0],
                    FLAT,
                    ONE,
                );
                let star = self.glint();
                self.part(root, facets(0.05, 4), &star, [0.0, 0.4, 0.21], FLAT, ONE);
            }
            "fedora" => {
                self.part(
                    root,
                    cylinder(0.26, 0.3, 0.32),
                    &skin,
                    [0.0, 0.2, 0.0],
                    FLAT,
                    [1.0, 1.0, 1.15],
                );
                self.part(
                    root,
                    dome(0.26),
                    &skin,
                    [0.0, 0.36, 0.0],
                    FLAT,
                    [1.0, 0.35, 1.15],
                );
                self.part(
                    root,
                    cylinder(0.3, 0.3, 0.07),
                    &dark,
                    [0.0, 0.09, 0.0],
                    FLAT,
                    [1.01, 1.0, 1.16],
                );
                self.part(
                    root,
                    cylinder(0.52, 0.52, 0.03),
                    &skin,
                    [0.0, 0.03, 0.0],
                    [4.0, 0.0, 0.0],
                    [1.0, 1.0, 1.1],
                );
            }
            "pot" => {
                self.part(
                    root,
                    cylinder(0.36, 0.32, 0.42),
                    &skin,
                    [0.0, 0.21, 0.0],
                    FLAT,
                    ONE,
                );
                self.part(root, torus(0.34, 0.4), &skin, [0.0, 0.42, 0.0], FLAT, ONE);
                self.part(
                    root,
                    cuboid(0.36, 0.05, 0.08),
                    &skin,
                    [0.5, 0.36, 0.0],
                    FLAT,
                    ONE,
                );
                self.part(
                    root,
                    sphere(0.1),
                    &dark,
                    [0.2, 0.28, 0.26],
                    FLAT,
                    [0.6, 1.0, 0.3],
                );
            }
            _ => {
                // a helmet: skull, rim, nose guard, and what its name says
                self.part(
                    root,
                    dome(0.4),
                    &skin,
                    [0.0, 0.1, 0.0],
                    FLAT,
                    [1.0, 1.35, 1.12],
                );
                self.part(
                    root,
                    torus(0.37, 0.45),
                    &skin,
                    [0.0, 0.1, 0.0],
                    FLAT,
                    [1.0, 1.0, 1.12],
                );
                self.part(
                    root,
                    cuboid(0.07, 0.3, 0.05),
                    &skin,
                    [0.0, 0.08, 0.46],
                    [-10.0, 0.0, 0.0],
                    ONE,
                );
                self.part(
                    root,
                    cuboid(0.62, 0.06, 0.04),
                    &dark,
                    [0.0, 0.3, 0.43],
                    [-8.0, 0.0, 0.0],
                    ONE,
                );
                match shape.as_str() {
                    "plumed" => {
                        let plume = self.mat(Color::from_rgb(0.62, 0.08, 0.06), Finish::Matte);
                        for (i, a) in [(0, -30.0f32), (1, -55.0), (2, -80.0)] {
                            self.part(
                                root,
                                capsule(0.07, 0.4),
                                &plume,
                                [0.0, 0.62 - i as f32 * 0.06, -0.12 - i as f32 * 0.14],
                                [a, 0.0, 0.0],
                                ONE,
                            );
                        }
                    }
                    "crested" => self.part(
                        root,
                        prism(0.06, 0.2, 0.7),
                        &skin,
                        [0.0, 0.6, -0.02],
                        FLAT,
                        ONE,
                    ),
                    "visored" => self.part(
                        root,
                        dome(0.42),
                        &skin,
                        [0.0, 0.12, 0.04],
                        [-80.0, 0.0, 0.0],
                        [1.0, 0.45, 0.95],
                    ),
                    "etched" => {
                        let gold = self.plain("gilded", Color::from_rgb(0.7, 0.55, 0.2));
                        for a in [-40.0f32, 0.0, 40.0] {
                            self.part(
                                root,
                                torus(0.395, 0.41),
                                &gold,
                                [0.0, 0.3, 0.0],
                                [a, 0.0, 90.0],
                                [1.0, 1.0, 1.35],
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// A pair of boots (or shoes), one a little behind the other.
    fn boots(&mut self, root: &mut Gd<Node3D>) {
        let skin = self.skin.clone();
        let sole = self.mat(Color::from_rgb(0.08, 0.06, 0.05), Finish::Matte);
        let shoe = self.shape_is("shoes");
        let shaft = if shoe { 0.16 } else { 0.55 };
        for (s, dz, yaw) in [(-1.0f32, -0.1f32, 12.0f32), (1.0, 0.08, -6.0)] {
            let mut boot = self.pivot(root, "Boot", [s * 0.19, 0.0, dz]);
            boot.set_rotation_degrees(Vector3::new(0.0, yaw, 0.0));
            self.part(
                &mut boot,
                cylinder(0.13, 0.14, shaft),
                &skin,
                [0.0, 0.1 + shaft * 0.5, -0.08],
                FLAT,
                ONE,
            );
            if !shoe {
                self.part(
                    &mut boot,
                    torus(0.12, 0.17),
                    &skin,
                    [0.0, 0.1 + shaft, -0.08],
                    FLAT,
                    [1.0, 1.4, 1.0],
                );
            }
            self.part(
                &mut boot,
                capsule(0.13, 0.5),
                &skin,
                [0.0, 0.12, 0.06],
                [90.0, 0.0, 0.0],
                [1.05, 1.0, 0.85],
            );
            self.part(
                &mut boot,
                cuboid(0.27, 0.05, 0.52),
                &sole,
                [0.0, 0.025, 0.06],
                FLAT,
                ONE,
            );
            self.part(
                &mut boot,
                cuboid(0.22, 0.09, 0.12),
                &sole,
                [0.0, 0.045, -0.13],
                FLAT,
                ONE,
            );
        }
    }

    /// A pair of gloves lying palm down, fingers forward.
    fn gloves(&mut self, root: &mut Gd<Node3D>) {
        let skin = self.skin.clone();
        let dark = self.dark();
        for (s, yaw, dz) in [(-1.0f32, 18.0f32, -0.06f32), (1.0, -12.0, 0.06)] {
            let mut g = self.pivot(root, "Glove", [s * 0.24, 0.0, dz]);
            g.set_rotation_degrees(Vector3::new(0.0, yaw, 0.0));
            self.part(
                &mut g,
                sphere(0.17),
                &skin,
                [0.0, 0.07, 0.0],
                FLAT,
                [1.0, 0.4, 1.2],
            );
            for f in 0..4 {
                let x = (f as f32 - 1.5) * 0.075;
                let l = [0.18, 0.22, 0.21, 0.16][f];
                self.part(
                    &mut g,
                    capsule(0.036, l),
                    &skin,
                    [x, 0.06, 0.17 + l * 0.4],
                    [90.0, 0.0, 0.0],
                    ONE,
                );
            }
            self.part(
                &mut g,
                capsule(0.04, 0.18),
                &skin,
                [-s * 0.16, 0.06, 0.07],
                [90.0, -s * 40.0, 0.0],
                ONE,
            );
            self.part(
                &mut g,
                cylinder(0.15, 0.18, 0.2),
                &skin,
                [0.0, 0.08, -0.25],
                [90.0, 0.0, 0.0],
                [1.0, 1.0, 0.55],
            );
            self.part(
                &mut g,
                torus(0.15, 0.19),
                &dark,
                [0.0, 0.08, -0.35],
                [90.0, 0.0, 0.0],
                [1.0, 1.0, 0.55],
            );
        }
    }

    /// Clothes: a hooded cloak, a cape, a robe, an apron, a shirt, a
    /// wrapping, a folded cloth, by the appearance.
    fn garment(&mut self, root: &mut Gd<Node3D>) {
        let skin = self.skin.clone();
        let dark = self.dark();
        let shape = self.shape.clone().unwrap_or_default();
        match shape.as_str() {
            "cloak" | "cape" | "robe" => {
                let hood = shape == "cloak";
                let len = if shape == "cape" { 0.7 } else { 0.95 };
                self.part(
                    root,
                    cylinder(0.16, 0.42, len),
                    &skin,
                    [0.0, 0.95 - len * 0.5, 0.0],
                    FLAT,
                    [1.0, 1.0, 0.5],
                );
                // the opening at the front
                self.part(
                    root,
                    cuboid(0.07, len * 0.8, 0.02),
                    &dark,
                    [0.0, 0.95 - len * 0.55, 0.205],
                    [-12.0, 0.0, 0.0],
                    ONE,
                );
                if shape == "robe" {
                    for s in [-1.0f32, 1.0] {
                        self.part(
                            root,
                            cylinder(0.08, 0.14, 0.45),
                            &skin,
                            [s * 0.3, 0.68, 0.0],
                            [0.0, 0.0, s * 28.0],
                            ONE,
                        );
                    }
                    self.part(
                        root,
                        cylinder(0.3, 0.3, 0.05),
                        &dark,
                        [0.0, 0.55, 0.0],
                        FLAT,
                        [0.95, 1.0, 0.5],
                    );
                }
                if hood {
                    self.part(
                        root,
                        sphere(0.19),
                        &skin,
                        [0.0, 0.98, -0.04],
                        FLAT,
                        [1.0, 1.1, 1.0],
                    );
                    self.part(
                        root,
                        sphere(0.13),
                        &dark,
                        [0.0, 0.97, 0.07],
                        FLAT,
                        [1.0, 1.1, 0.7],
                    );
                } else {
                    self.part(
                        root,
                        torus(0.14, 0.21),
                        &skin,
                        [0.0, 0.93, 0.0],
                        FLAT,
                        [1.0, 1.6, 0.8],
                    );
                }
                let clasp = self.plain("gilded", Color::from_rgb(0.7, 0.55, 0.2));
                self.part(root, sphere(0.035), &clasp, [0.0, 0.88, 0.1], FLAT, ONE);
            }
            "apron" => {
                self.part(
                    root,
                    cuboid(0.5, 0.7, 0.03),
                    &skin,
                    [0.0, 0.4, 0.0],
                    FLAT,
                    ONE,
                );
                self.part(
                    root,
                    cuboid(0.3, 0.12, 0.035),
                    &dark,
                    [0.0, 0.3, 0.005],
                    FLAT,
                    ONE,
                );
                self.part(
                    root,
                    torus(0.13, 0.16),
                    &skin,
                    [0.0, 0.82, 0.0],
                    [90.0, 0.0, 0.0],
                    [1.0, 1.0, 1.4],
                );
                for s in [-1.0f32, 1.0] {
                    self.part(
                        root,
                        cuboid(0.3, 0.03, 0.02),
                        &skin,
                        [s * 0.35, 0.56, 0.0],
                        [0.0, 0.0, s * -20.0],
                        ONE,
                    );
                }
            }
            "shirt" => {
                self.part(
                    root,
                    cuboid(0.5, 0.62, 0.12),
                    &skin,
                    [0.0, 0.36, 0.0],
                    FLAT,
                    ONE,
                );
                for s in [-1.0f32, 1.0] {
                    self.part(
                        root,
                        cuboid(0.28, 0.2, 0.11),
                        &skin,
                        [s * 0.33, 0.58, 0.0],
                        [0.0, 0.0, s * -32.0],
                        ONE,
                    );
                }
                self.part(
                    root,
                    cylinder(0.1, 0.1, 0.02),
                    &dark,
                    [0.0, 0.67, 0.0],
                    FLAT,
                    [1.0, 1.0, 0.8],
                );
                if self.tint.r > self.tint.b {
                    // a loud print
                    let flower = self.mat(Color::from_rgb(0.95, 0.85, 0.3), Finish::Matte);
                    for (x, y) in [
                        (-0.14f32, 0.5f32),
                        (0.12, 0.35),
                        (-0.06, 0.18),
                        (0.16, 0.56),
                    ] {
                        self.part(
                            root,
                            sphere(0.045),
                            &flower,
                            [x, y, 0.06],
                            FLAT,
                            [1.0, 1.0, 0.3],
                        );
                    }
                }
            }
            "wrapping" => {
                for (i, r) in [(0, 0.42f32), (1, 0.36), (2, 0.3), (3, 0.22)] {
                    self.part(
                        root,
                        torus(r - 0.09, r),
                        &skin,
                        [0.0, 0.05 + i as f32 * 0.05, 0.0],
                        FLAT,
                        [1.0, 0.8, 1.0],
                    );
                }
                self.part(
                    root,
                    cuboid(0.14, 0.02, 0.5),
                    &skin,
                    [0.3, 0.01, 0.4],
                    [0.0, 35.0, 0.0],
                    ONE,
                );
                for a in [20.0f32, 80.0, 140.0] {
                    self.part(
                        root,
                        cuboid(0.02, 0.1, 0.12),
                        &dark,
                        [0.4 * a.to_radians().cos(), 0.08, 0.4 * a.to_radians().sin()],
                        [0.0, -a, 0.0],
                        ONE,
                    );
                }
            }
            // a folded cloth
            _ => {
                for (i, a) in [(0, 5.0f32), (1, -8.0), (2, 3.0)] {
                    let w = 0.9 - i as f32 * 0.08;
                    self.part(
                        root,
                        cuboid(w, 0.07, w * 0.7),
                        &skin,
                        [0.0, 0.035 + i as f32 * 0.075, 0.0],
                        [0.0, a, 0.0],
                        ONE,
                    );
                }
                self.part(
                    root,
                    capsule(0.05, 0.75),
                    &skin,
                    [0.0, 0.2, 0.28],
                    [0.0, 0.0, 90.0],
                    ONE,
                );
            }
        }
    }

    /// Body armour standing up: plate with pauldrons and tassets, mail in
    /// rings, scales in rows, leather with straps, or loose dragon scales.
    fn cuirass(&mut self, root: &mut Gd<Node3D>) {
        let skin = self.skin.clone();
        let dark = self.dark();
        let leather = self.leather();
        let shape = self.shape.clone().unwrap_or_default();
        if shape == "scales" {
            let c = vivid(self.tint, 0.75);
            let scale = self.mat(c, Finish::Glossy);
            let ridge = self.mat(vivid(self.tint, 0.45), Finish::Glossy);
            for row in 0..3 {
                for col in 0..(3 - row) {
                    let x = (col as f32 - (2 - row) as f32 * 0.5) * 0.34;
                    let (y, z) = (0.06 + row as f32 * 0.07, 0.25 - row as f32 * 0.24);
                    self.part(
                        root,
                        sphere(0.2),
                        &scale,
                        [x, y, z],
                        [-18.0, 0.0, 0.0],
                        [0.85, 0.16, 1.1],
                    );
                    self.part(
                        root,
                        cuboid(0.03, 0.03, 0.3),
                        &ridge,
                        [x, y + 0.03, z],
                        [-18.0, 0.0, 0.0],
                        ONE,
                    );
                }
            }
            return;
        }
        let body_mat = if shape == "dragon" {
            let c = vivid(self.tint, 0.75);
            self.mat(c, Finish::Glossy)
        } else {
            skin.clone()
        };
        self.part(
            root,
            cylinder(0.34, 0.28, 0.6),
            &body_mat,
            [0.0, 0.44, 0.0],
            FLAT,
            [1.0, 1.0, 0.62],
        );
        self.part(
            root,
            sphere(0.3),
            &body_mat,
            [0.0, 0.56, 0.06],
            FLAT,
            [1.1, 0.8, 0.62],
        );
        self.part(
            root,
            cylinder(0.14, 0.15, 0.03),
            &dark,
            [0.0, 0.75, 0.0],
            FLAT,
            [1.0, 1.0, 0.7],
        );
        self.part(
            root,
            cylinder(0.3, 0.3, 0.07),
            &leather,
            [0.0, 0.18, 0.0],
            FLAT,
            [1.0, 1.0, 0.64],
        );
        match shape.as_str() {
            "mail" => {
                for i in 0..6 {
                    let y = 0.26 + i as f32 * 0.085;
                    let r = 0.286 + i as f32 * 0.009;
                    self.part(
                        root,
                        cylinder(r, r, 0.012),
                        &dark,
                        [0.0, y, 0.0],
                        FLAT,
                        [1.02, 1.0, 0.64],
                    );
                }
                for s in [-1.0f32, 1.0] {
                    self.part(
                        root,
                        cylinder(0.1, 0.13, 0.26),
                        &skin,
                        [s * 0.36, 0.62, 0.0],
                        [0.0, 0.0, s * 30.0],
                        ONE,
                    );
                }
                self.part(
                    root,
                    cylinder(0.3, 0.34, 0.16),
                    &skin,
                    [0.0, 0.08, 0.0],
                    FLAT,
                    [1.0, 1.0, 0.62],
                );
            }
            "scale" | "dragon" => {
                let scale = body_mat.clone();
                for row in 0..4 {
                    for c in 0..5 {
                        let x = (c as f32 - 2.0) * 0.11 + if row % 2 == 1 { 0.055 } else { 0.0 };
                        let y = 0.3 + row as f32 * 0.1;
                        self.part(
                            root,
                            sphere(0.065),
                            &scale,
                            [x, y, 0.2 - x.abs() * 0.25],
                            [30.0, 0.0, 0.0],
                            [1.0, 1.1, 0.4],
                        );
                    }
                }
                for s in [-1.0f32, 1.0] {
                    self.part(
                        root,
                        dome(0.16),
                        &scale,
                        [s * 0.36, 0.68, 0.0],
                        [0.0, 0.0, s * -35.0],
                        [1.0, 0.8, 1.1],
                    );
                }
            }
            "leather" | "studded" => {
                for s in [-1.0f32, 1.0] {
                    self.part(
                        root,
                        cuboid(0.07, 0.3, 0.3),
                        &leather,
                        [s * 0.16, 0.72, 0.0],
                        FLAT,
                        ONE,
                    );
                }
                let studs = self.plain("metal", Color::from_rgb(0.6, 0.6, 0.62));
                if self.shape_is("studded") {
                    for (x, y) in [
                        (-0.14f32, 0.4f32),
                        (0.0, 0.33),
                        (0.14, 0.4),
                        (-0.12, 0.56),
                        (0.12, 0.56),
                    ] {
                        self.part(root, sphere(0.02), &studs, [x, y, 0.2], FLAT, ONE);
                    }
                }
            }
            // plate
            _ => {
                for s in [-1.0f32, 1.0] {
                    self.part(
                        root,
                        dome(0.17),
                        &skin,
                        [s * 0.36, 0.68, 0.0],
                        [0.0, 0.0, s * -35.0],
                        [1.0, 0.8, 1.1],
                    );
                    self.part(
                        root,
                        dome(0.14),
                        &skin,
                        [s * 0.4, 0.62, 0.0],
                        [0.0, 0.0, s * -50.0],
                        [1.0, 0.7, 1.0],
                    );
                }
                self.part(
                    root,
                    cuboid(0.02, 0.4, 0.02),
                    &dark,
                    [0.0, 0.5, 0.205],
                    [8.0, 0.0, 0.0],
                    ONE,
                );
                for (x, a) in [(-0.16f32, 8.0f32), (0.0, 0.0), (0.16, -8.0)] {
                    self.part(
                        root,
                        cuboid(0.15, 0.17, 0.035),
                        &skin,
                        [x, 0.07, 0.15],
                        [-6.0, 0.0, a],
                        ONE,
                    );
                }
            }
        }
    }

    /// A bow lying flat: a curved stave, a wrapped grip, the string.
    fn bow(&mut self, root: &mut Gd<Node3D>) {
        let skin = self.skin.clone();
        if self.shape_is("boomerang") {
            for s in [-1.0f32, 1.0] {
                self.part(
                    root,
                    capsule(0.07, 0.6),
                    &skin,
                    [s * 0.2, 0.03, 0.1],
                    [90.0, s * 55.0, 0.0],
                    [1.0, 1.0, 0.35],
                );
            }
            return;
        }
        let (r, n) = (0.62f32, 9);
        let (a0, a1) = (-58f32.to_radians(), 58f32.to_radians());
        let seg = r * (a1 - a0) / n as f32 + 0.02;
        for i in 0..n {
            let a = a0 + (a1 - a0) * (i as f32 + 0.5) / n as f32;
            let thick = 0.028 - (a.abs() / a1) * 0.01;
            self.part(
                root,
                cylinder(thick, thick, seg),
                &skin,
                [r * a.cos() - r * 0.72, 0.03, r * a.sin()],
                [90.0, -a.to_degrees(), 0.0],
                ONE,
            );
        }
        let grip = self.leather();
        self.part(
            root,
            cylinder(0.036, 0.036, 0.16),
            &grip,
            [r - r * 0.72, 0.03, 0.0],
            [90.0, 0.0, 0.0],
            ONE,
        );
        let string = self.bone();
        let tip_x = r * a1.cos() - r * 0.72;
        let tip_z = r * a1.sin();
        self.part(
            root,
            cylinder(0.005, 0.005, tip_z * 2.0),
            &string,
            [tip_x, 0.03, 0.0],
            [90.0, 0.0, 0.0],
            ONE,
        );
    }

    /// A horn: the unicorn's spiral, a curved horn, a bugle, a flute.
    fn horn(&mut self, root: &mut Gd<Node3D>) {
        let skin = self.skin.clone();
        let dark = self.dark();
        let shape = self.shape.clone().unwrap_or_default();
        match shape.as_str() {
            "curved" => {
                for i in 0..6 {
                    let t = i as f32 / 5.0;
                    let a = t * 70.0;
                    let r = 0.03 + t * 0.09;
                    self.part(
                        root,
                        cylinder(r + 0.015, r, 0.2),
                        &skin,
                        [
                            -0.3 + t * 0.55 + (a.to_radians().sin() * 0.05),
                            0.12,
                            -0.35 + t * 0.6 - t * t * 0.3,
                        ],
                        [90.0, 20.0 + a, 0.0],
                        ONE,
                    );
                }
                let brass = self.plain("gilded", Color::from_rgb(0.7, 0.55, 0.2));
                self.part(
                    root,
                    cylinder(0.04, 0.04, 0.08),
                    &brass,
                    [-0.32, 0.12, -0.38],
                    [90.0, 20.0, 0.0],
                    ONE,
                );
            }
            "bugle" => {
                self.part(
                    root,
                    cylinder(0.025, 0.025, 0.7),
                    &skin,
                    [0.0, 0.05, -0.05],
                    [90.0, 0.0, 0.0],
                    ONE,
                );
                self.part(
                    root,
                    cylinder(0.2, 0.03, 0.3),
                    &skin,
                    [0.0, 0.05, 0.4],
                    [90.0, 0.0, 0.0],
                    ONE,
                );
                self.part(
                    root,
                    torus(0.08, 0.11),
                    &skin,
                    [0.0, 0.05, -0.1],
                    [0.0, 0.0, 90.0],
                    [1.0, 1.0, 2.0],
                );
            }
            "flute" => {
                self.part(
                    root,
                    cylinder(0.035, 0.035, 0.95),
                    &skin,
                    [0.0, 0.035, 0.0],
                    [90.0, 0.0, 0.0],
                    ONE,
                );
                for i in 0..6 {
                    self.part(
                        root,
                        cylinder(0.02, 0.02, 0.012),
                        &dark,
                        [0.0, 0.068, -0.1 + i as f32 * 0.08],
                        FLAT,
                        ONE,
                    );
                }
            }
            _ => {
                self.skin(
                    root,
                    cylinder(0.0, 0.1, 1.0),
                    [0.0, 0.08, 0.0],
                    [90.0, 0.0, 0.0],
                    ONE,
                );
                for z in [-0.3f32, -0.15, 0.0, 0.15, 0.3] {
                    self.part(
                        root,
                        torus(0.05 - z * 0.08, 0.065 - z * 0.09),
                        &dark,
                        [0.0, 0.08, z],
                        [90.0, 0.0, 0.0],
                        ONE,
                    );
                }
            }
        }
    }

    /// The rest, as the kit always built them.
    fn simple(&mut self, kind: Proc, root: &mut Gd<Node3D>) {
        let one = ONE;
        let flat = FLAT;
        match kind {
            Proc::Rock => {
                self.skin(
                    root,
                    facets(0.5, 7),
                    [0.0, 0.35, 0.0],
                    [10.0, 30.0, 0.0],
                    [1.1, 0.7, 0.9],
                );
            }
            Proc::Ball => {
                self.skin(root, sphere(0.45), [0.0, 0.45, 0.0], flat, one);
                self.skin(
                    root,
                    torus(0.08, 0.14),
                    [0.4, 0.06, 0.35],
                    [0.0, 30.0, 0.0],
                    one,
                );
                self.skin(
                    root,
                    torus(0.08, 0.14),
                    [0.58, 0.06, 0.52],
                    [90.0, 30.0, 0.0],
                    one,
                );
            }
            Proc::Pole => {
                self.skin(
                    root,
                    cylinder(0.018, 0.022, 0.85),
                    [0.0, 0.03, -0.07],
                    [90.0, 0.0, 0.0],
                    one,
                );
                let metal = self.named("metal", Color::from_rgb(0.6, 0.6, 0.62));
                self.part(
                    root,
                    cylinder(0.0, 0.04, 0.16),
                    &metal,
                    [0.0, 0.03, 0.43],
                    [90.0, 0.0, 0.0],
                    one,
                );
                self.part(
                    root,
                    cuboid(0.1, 0.02, 0.05),
                    &metal,
                    [0.0, 0.03, 0.34],
                    flat,
                    one,
                );
            }
            Proc::Arrows => {
                let metal = self.named("metal", Color::from_rgb(0.6, 0.6, 0.62));
                let feather = self.bone();
                for (x, a) in [(-0.06f32, -6.0f32), (0.0, 2.0), (0.06, 8.0)] {
                    self.skin(
                        root,
                        cylinder(0.008, 0.008, 0.9),
                        [x, 0.02, 0.0],
                        [90.0, a, 0.0],
                        one,
                    );
                    self.part(
                        root,
                        cylinder(0.0, 0.02, 0.08),
                        &metal,
                        [x + a * 0.004, 0.02, 0.48],
                        [90.0, a, 0.0],
                        one,
                    );
                    self.part(
                        root,
                        prism(0.04, 0.12, 0.01),
                        &feather,
                        [x - a * 0.004, 0.03, -0.42],
                        [90.0, a, 0.0],
                        one,
                    );
                }
            }
            Proc::Fruit => {
                let peel = self.mat(vivid(self.tint, 0.85), Finish::Glossy);
                self.part(
                    root,
                    sphere(0.45),
                    &peel,
                    [0.0, 0.42, 0.0],
                    flat,
                    [1.0, 0.92, 1.0],
                );
                let stem = self.mat(Color::from_rgb(0.25, 0.18, 0.08), Finish::Matte);
                self.part(
                    root,
                    cylinder(0.02, 0.03, 0.2),
                    &stem,
                    [0.0, 0.9, 0.0],
                    [0.0, 0.0, 12.0],
                    one,
                );
                let leaf = self.mat(Color::from_rgb(0.2, 0.4, 0.12), Finish::Matte);
                self.part(
                    root,
                    sphere(0.1),
                    &leaf,
                    [0.09, 0.9, 0.0],
                    [0.0, 0.0, -30.0],
                    [1.0, 0.25, 0.5],
                );
            }
            Proc::Egg => {
                self.skin(root, sphere(0.36), [0.0, 0.48, 0.0], flat, [1.0, 1.35, 1.0]);
            }
            Proc::Tin => {
                self.skin(root, cylinder(0.4, 0.4, 0.55), [0.0, 0.28, 0.0], flat, one);
                let label = self.mat(Color::from_rgb(0.55, 0.12, 0.08), Finish::Matte);
                self.part(
                    root,
                    cylinder(0.41, 0.41, 0.3),
                    &label,
                    [0.0, 0.28, 0.0],
                    flat,
                    one,
                );
                let dark = self.dark();
                for y in [0.02f32, 0.54] {
                    self.part(
                        root,
                        torus(0.37, 0.41),
                        &dark,
                        [0.0, y, 0.0],
                        flat,
                        [1.0, 0.5, 1.0],
                    );
                }
            }
            Proc::Lump => {
                self.skin(root, sphere(0.4), [0.0, 0.3, 0.0], flat, [1.2, 0.7, 1.0]);
                self.skin(root, sphere(0.25), [0.25, 0.22, 0.2], flat, [1.0, 0.8, 1.0]);
            }
            Proc::Splash => {
                self.skin(
                    root,
                    cylinder(0.5, 0.5, 0.02),
                    [0.0, 0.01, 0.0],
                    flat,
                    [1.0, 1.0, 0.8],
                );
                self.skin(root, sphere(0.12), [0.3, 0.03, 0.25], flat, [1.0, 0.3, 1.0]);
            }
            Proc::Orb => {
                self.skin(root, sphere(0.4), [0.0, 0.5, 0.0], flat, one);
                let stand = self.named("gilded", Color::from_rgb(0.7, 0.55, 0.2));
                self.part(
                    root,
                    cylinder(0.2, 0.3, 0.15),
                    &stand,
                    [0.0, 0.075, 0.0],
                    flat,
                    one,
                );
            }
            Proc::Mirror => {
                self.skin(root, cylinder(0.3, 0.3, 0.04), [0.0, 0.03, 0.15], flat, one);
                let glass = self.mat(Color::from_rgb(0.75, 0.82, 0.88), Finish::Glossy);
                self.part(
                    root,
                    cylinder(0.26, 0.26, 0.045),
                    &glass,
                    [0.0, 0.035, 0.15],
                    flat,
                    one,
                );
                self.skin(
                    root,
                    cylinder(0.04, 0.04, 0.4),
                    [0.0, 0.03, -0.32],
                    [90.0, 0.0, 0.0],
                    one,
                );
            }
            // a pile of things too many to tell apart
            _ => {
                let wood = self.named("wood", Color::from_rgb(0.4, 0.28, 0.16));
                self.skin(
                    root,
                    cuboid(0.8, 0.12, 0.6),
                    [0.0, 0.06, 0.0],
                    [0.0, 10.0, 0.0],
                    one,
                );
                self.part(
                    root,
                    cuboid(0.5, 0.1, 0.4),
                    &wood,
                    [0.1, 0.17, -0.05],
                    [0.0, -25.0, 0.0],
                    one,
                );
                self.skin(root, sphere(0.2), [-0.2, 0.2, 0.15], flat, [1.0, 0.6, 1.0]);
            }
        }
    }
}
