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

fn xyz(v: Vector3) -> [f32; 3] {
    [v.x, v.y, v.z]
}

/// The rotation (degrees, as `part` takes it) that turns a part's +y to
/// `d`.
fn along(d: Vector3) -> [f32; 3] {
    let d = d.normalized();
    [
        d.y.clamp(-1.0, 1.0).acos().to_degrees(),
        d.x.atan2(d.z).to_degrees(),
        0.0,
    ]
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
            Proc::Pole => self.pole(root),
            Proc::Mace => self.mace(root),
            Proc::Sword => self.sword(root),
            Proc::Ration => self.ration(root),
            Proc::Carried => self.carried(root),
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
        // a dark seal over the cork: a red one read as an eye on a pale flask
        let wax = self.mat(Color::from_rgb(0.26, 0.15, 0.09), Finish::Glossy);
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
            "pointed" => {
                // a wizard's: tall, its tip bent back a little, a brim
                // narrow enough to leave the face to the camera above
                self.part(
                    root,
                    cylinder(0.12, 0.32, 0.8),
                    &skin,
                    [0.0, 0.42, 0.0],
                    FLAT,
                    ONE,
                );
                self.part(
                    root,
                    cylinder(0.0, 0.12, 0.45),
                    &skin,
                    [0.0, 0.98, -0.06],
                    [-18.0, 0.0, 0.0],
                    ONE,
                );
                self.part(
                    root,
                    cylinder(0.43, 0.43, 0.03),
                    &skin,
                    [0.0, 0.03, 0.0],
                    FLAT,
                    ONE,
                );
                self.part(
                    root,
                    cylinder(0.33, 0.33, 0.08),
                    &dark,
                    [0.0, 0.09, 0.0],
                    FLAT,
                    ONE,
                );
            }
            "kabuto" => {
                // a samurai's: a lacquered bowl, the neck guard flaring out
                // in plates behind and at the sides, turned-back flaps at
                // the front and a gilded crest of two horns
                let gold = self.plain("gilded", Color::from_rgb(0.75, 0.58, 0.2));
                self.part(
                    root,
                    dome(0.4),
                    &skin,
                    [0.0, 0.1, 0.0],
                    FLAT,
                    [1.0, 1.2, 1.1],
                );
                self.part(
                    root,
                    torus(0.38, 0.45),
                    &gold,
                    [0.0, 0.1, 0.0],
                    FLAT,
                    [1.0, 1.0, 1.1],
                );
                for (y, k) in [(0.02f32, 1.0f32), (-0.1, 1.12), (-0.22, 1.24)] {
                    // back
                    self.part(
                        root,
                        cuboid(0.8 * k, 0.13, 0.04),
                        &skin,
                        [0.0, y, -0.44 * k],
                        [-28.0, 0.0, 0.0],
                        ONE,
                    );
                    for side in [-1.0f32, 1.0] {
                        self.part(
                            root,
                            cuboid(0.04, 0.13, 0.42 * k),
                            &skin,
                            [side * 0.44 * k, y, -0.14],
                            [0.0, 0.0, side * 28.0],
                            ONE,
                        );
                    }
                }
                for side in [-1.0f32, 1.0] {
                    self.part(
                        root,
                        cuboid(0.2, 0.16, 0.03),
                        &skin,
                        [side * 0.42, 0.14, 0.24],
                        [0.0, side * 55.0, 0.0],
                        ONE,
                    );
                    self.part(
                        root,
                        cuboid(0.05, 0.5, 0.02),
                        &gold,
                        [side * 0.14, 0.52, 0.42],
                        [-10.0, 0.0, -side * 24.0],
                        ONE,
                    );
                }
                self.part(
                    root,
                    cylinder(0.07, 0.07, 0.03),
                    &gold,
                    [0.0, 0.3, 0.44],
                    [90.0, 0.0, 0.0],
                    ONE,
                );
            }
            "mask" => {
                // a band of black cloth across the eyes, knotted behind
                let dark = self.mat(Color::from_rgb(0.03, 0.03, 0.035), Finish::Matte);
                self.part(
                    root,
                    torus(0.4, 0.47),
                    &dark,
                    [0.0, 0.25, 0.02],
                    FLAT,
                    [1.0, 2.2, 1.1],
                );
                for s in [-1.0f32, 1.0] {
                    self.part(
                        root,
                        cuboid(0.05, 0.2, 0.02),
                        &dark,
                        [s * 0.05, 0.14, -0.5],
                        [0.0, 0.0, s * 20.0],
                        ONE,
                    );
                }
            }
            "snakes" => self.snakes(root),
            // a knight's plume alone, for a helmet already worn
            "plume" => self.plume(root),
            "eyes" => {
                // eyes that glow out of a hood's shadow
                let glow = self.mat(vivid(self.tint, 1.4), Finish::Glow);
                for side in [-1.0f32, 1.0] {
                    self.part(
                        root,
                        sphere(0.06),
                        &glow,
                        [side * 0.14, 0.0, 0.0],
                        FLAT,
                        [1.4, 0.8, 0.5],
                    );
                }
            }
            "halo" => {
                // a ring of light standing behind the head
                let glow = self.mat(vivid(self.tint, 1.3), Finish::Ember);
                self.part(
                    root,
                    torus(0.66, 0.74),
                    &glow,
                    [0.0, 0.4, -0.4],
                    [90.0, 0.0, 0.0],
                    ONE,
                );
            }
            "circlet" => {
                // a thin gold band round the brow, a stone at the front
                let gold = self.plain("gilded", Color::from_rgb(0.8, 0.62, 0.22));
                self.part(
                    root,
                    torus(0.4, 0.44),
                    &gold,
                    [0.0, 0.0, 0.0],
                    [-6.0, 0.0, 0.0],
                    [1.0, 1.6, 1.12],
                );
                let stone = self.gem_mat();
                self.cut_stone(root, &stone, [0.0, -0.05, 0.47], 0.08);
            }
            "cap" => {
                // a gnome's: a tall cone with no brim, its tip flopped
                // back, a rolled band at the brow
                self.part(
                    root,
                    cylinder(0.14, 0.33, 0.62),
                    &skin,
                    [0.0, 0.33, 0.0],
                    [-5.0, 0.0, 0.0],
                    ONE,
                );
                self.part(
                    root,
                    cylinder(0.0, 0.14, 0.44),
                    &skin,
                    [0.0, 0.8, -0.1],
                    [-30.0, 0.0, 0.0],
                    ONE,
                );
                self.part(
                    root,
                    torus(0.27, 0.37),
                    &dark,
                    [0.0, 0.04, 0.0],
                    FLAT,
                    [1.0, 1.5, 1.0],
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
                    cylinder(0.44, 0.44, 0.03),
                    &skin,
                    [0.0, 0.03, 0.0],
                    // its front snapped up: the face shows from above
                    [-8.0, 0.0, 0.0],
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
                    "winged" => {
                        // a Valkyrie's: a fan of white feathers splayed out
                        // on each side, facing forward (they read as wings
                        // from the camera)
                        let wing = self.mat(Color::from_rgb(0.86, 0.84, 0.78), Finish::Matte);
                        for side in [-1.0f32, 1.0] {
                            for (i, a) in [(0.0f32, 18.0f32), (1.0, 40.0), (2.0, 62.0)] {
                                self.part(
                                    root,
                                    prism(0.16, 0.55 - i * 0.08, 0.025),
                                    &wing,
                                    [side * (0.5 + i * 0.05), 0.42 - i * 0.06, -0.02 - i * 0.03],
                                    [0.0, 0.0, -side * a],
                                    ONE,
                                );
                            }
                        }
                    }
                    "plumed" => self.plume(root),
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

    /// A knight's plume: red feathers sweeping back from the crown.
    fn plume(&mut self, root: &mut Gd<Node3D>) {
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

    /// A gorgon's hair: serpents from the scalp writhing out and up, each
    /// a tapering chain ending in a head; none hangs over the face (+z).
    /// About ninety parts: the map has one Medusa.
    fn snakes(&mut self, root: &mut Gd<Node3D>) {
        let scales = self.skin.clone();
        let belly = self.mat(
            Color::from_rgb(
                (self.tint.r * 1.2 + 0.1).min(1.0),
                (self.tint.g * 1.2 + 0.12).min(1.0),
                (self.tint.b * 1.1 + 0.03).min(1.0),
            ),
            Finish::Matte,
        );
        // three rings from the crown down; the lower two leave the face
        let rings: [(usize, f32, f32); 3] = [(6, 1.15, 0.6), (8, 0.65, 0.35), (8, 0.15, -0.1)];
        for (ring, &(n, e, rise)) in rings.iter().enumerate() {
            for i in 0..n {
                let a = (i as f32 + 0.5 * ring as f32) / n as f32 * std::f32::consts::TAU;
                if ring > 0 && a.cos() > 0.75 {
                    continue;
                }
                self.snake(root, &scales, &belly, a, e, rise, i + ring);
            }
        }
    }

    /// One of a gorgon's snakes, from the scalp at azimuth `a` (from the
    /// face, +z) and elevation `e`, rising `rise`: four tapering
    /// segments, scales and belly by turns, and a flattened head.
    #[allow(clippy::too_many_arguments)]
    fn snake(
        &mut self,
        root: &mut Gd<Node3D>,
        scales: &Gd<Material>,
        belly: &Gd<Material>,
        a: f32,
        e: f32,
        rise: f32,
        i: usize,
    ) {
        let out = Vector3::new(a.sin() * e.cos(), e.sin(), a.cos() * e.cos());
        let side = Vector3::UP.cross(out).normalized();
        let mut p = Vector3::new(0.0, 0.1, 0.0) + out * 0.34;
        let mut d = (out + Vector3::UP * rise).normalized();
        let mut r = 0.06f32;
        for s in 0..4 {
            let wave = if (s + i).is_multiple_of(2) {
                0.55
            } else {
                -0.55
            };
            d = (d + side * wave + Vector3::DOWN * (0.12 * s as f32)).normalized();
            let len = 0.16;
            let mat = if s.is_multiple_of(2) { scales } else { belly };
            self.part(
                root,
                capsule(r, len + 2.0 * r),
                mat,
                xyz(p + d * (len * 0.5)),
                along(d),
                ONE,
            );
            p += d * len;
            r *= 0.86;
        }
        self.part(
            root,
            sphere(0.07),
            scales,
            xyz(p + d * 0.04),
            along(d),
            [1.1, 1.4, 0.75],
        );
    }

    /// Small things carried, by shape: a camera on its strap, a seer's
    /// crystal glowing in its own light, a hand bell.
    fn carried(&mut self, root: &mut Gd<Node3D>) {
        match self.shape.as_deref() {
            Some("camera") => {
                let body = self.mat(Color::from_rgb(0.07, 0.07, 0.08), Finish::Glossy);
                let chrome = self.mat(Color::from_rgb(0.78, 0.8, 0.84), Finish::Glossy);
                let glass = self.mat(Color::from_rgb(0.25, 0.4, 0.6), Finish::Gem);
                self.part(
                    root,
                    cuboid(0.9, 0.5, 0.32),
                    &body,
                    [0.0, 0.25, 0.0],
                    FLAT,
                    ONE,
                );
                self.part(
                    root,
                    cuboid(0.92, 0.1, 0.34),
                    &chrome,
                    [0.0, 0.52, 0.0],
                    FLAT,
                    ONE,
                );
                self.part(
                    root,
                    cylinder(0.2, 0.23, 0.32),
                    &body,
                    [0.05, 0.25, 0.3],
                    [90.0, 0.0, 0.0],
                    ONE,
                );
                self.part(
                    root,
                    cylinder(0.16, 0.16, 0.03),
                    &glass,
                    [0.05, 0.25, 0.47],
                    [90.0, 0.0, 0.0],
                    ONE,
                );
                self.part(
                    root,
                    cuboid(0.22, 0.14, 0.16),
                    &chrome,
                    [-0.3, 0.62, 0.0],
                    FLAT,
                    ONE,
                );
            }
            Some("bell") => {
                // a hand bell, mouth down: a flared body, a rolled lip, a
                // domed crown with a loop to hold it by, the clapper
                // showing under the lip
                let metal = self.skin.clone();
                let dark = self.dark();
                self.part(
                    root,
                    cylinder(0.2, 0.4, 0.5),
                    &metal,
                    [0.0, 0.33, 0.0],
                    FLAT,
                    ONE,
                );
                self.part(
                    root,
                    torus(0.36, 0.46),
                    &metal,
                    [0.0, 0.08, 0.0],
                    FLAT,
                    [1.0, 1.8, 1.0],
                );
                self.part(root, dome(0.21), &metal, [0.0, 0.57, 0.0], FLAT, ONE);
                self.part(
                    root,
                    torus(0.07, 0.11),
                    &metal,
                    [0.0, 0.84, 0.0],
                    [90.0, 0.0, 0.0],
                    ONE,
                );
                self.part(root, sphere(0.09), &dark, [0.0, 0.06, 0.0], FLAT, ONE);
            }
            Some("crystal") => {
                let glow = self.mat(vivid(self.tint, 1.25), Finish::Ember);
                self.part(root, sphere(0.5), &glow, [0.0, 0.5, 0.0], FLAT, ONE);
            }
            _ => {}
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
            "mantle" => {
                // a short fur mantle on the shoulders: a thick ring of fur
                // and tufts
                self.part(
                    root,
                    torus(0.2, 0.5),
                    &skin,
                    [0.0, 0.1, 0.0],
                    FLAT,
                    [1.0, 0.6, 0.8],
                );
                for i in 0..10 {
                    let a = i as f32 * 36.0f32;
                    let (sn, cs) = a.to_radians().sin_cos();
                    self.part(
                        root,
                        sphere(0.12),
                        &skin,
                        [sn * 0.38, 0.14, cs * 0.3],
                        FLAT,
                        [1.0, 0.8, 1.0],
                    );
                }
            }
            "sash" => {
                // a band from one shoulder across the chest to the other hip
                self.part(
                    root,
                    torus(0.36, 0.52),
                    &skin,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 38.0],
                    [1.0, 1.0, 0.72],
                );
            }
            "sode" => {
                // a samurai's shoulder guards: lacquered plates laced in
                // rows, hanging out over each upper arm
                for side in [-1.0f32, 1.0] {
                    for (i, k) in [(0.0f32, 1.0f32), (1.0, 1.08), (2.0, 1.16)] {
                        self.part(
                            root,
                            cuboid(0.3 * k, 0.13, 0.42 * k),
                            &skin,
                            [side * (0.5 + i * 0.07), 0.38 - i * 0.12, 0.0],
                            [0.0, 0.0, side * -32.0],
                            ONE,
                        );
                        self.part(
                            root,
                            cuboid(0.31 * k, 0.02, 0.43 * k),
                            &dark,
                            [side * (0.5 + i * 0.07), 0.32 - i * 0.12, 0.0],
                            [0.0, 0.0, side * -32.0],
                            ONE,
                        );
                    }
                }
            }
            "obi" => {
                // a broad sash round the waist, its ends hanging in front
                self.part(
                    root,
                    torus(0.43, 0.5),
                    &skin,
                    [0.0, 0.0, 0.0],
                    FLAT,
                    [1.0, 3.2, 0.78],
                );
                for (x, a) in [(0.1f32, 8.0f32), (0.2, 16.0)] {
                    self.part(
                        root,
                        cuboid(0.09, 0.42, 0.02),
                        &skin,
                        [x, -0.2, 0.4],
                        [0.0, 0.0, a],
                        ONE,
                    );
                }
            }
            "hakama" | "loincloth" => {
                // wide trousers like a pleated skirt to the ankles, or a
                // short ragged pelt round the hips
                let long = shape == "hakama";
                let (top, len, hem) = if long {
                    (0.36, 1.6, 0.66)
                } else {
                    (0.42, 0.7, 0.6)
                };
                self.part(
                    root,
                    cylinder(top, hem, len),
                    &skin,
                    [0.0, 0.08 - len * 0.5, 0.0],
                    FLAT,
                    [1.0, 1.0, 0.8],
                );
                if long {
                    // its pleats
                    for i in 0..8 {
                        let a = i as f32 * 45.0 + 22.5;
                        let (sn, cs) = a.to_radians().sin_cos();
                        self.part(
                            root,
                            cuboid(0.02, len * 0.9, 0.04),
                            &dark,
                            [sn * 0.5, 0.08 - len * 0.52, cs * 0.4],
                            [0.0, a, sn * 7.0],
                            ONE,
                        );
                    }
                } else {
                    // the pelt's ragged hem
                    for i in 0..9 {
                        let a = i as f32 * 40.0;
                        let (sn, cs) = a.to_radians().sin_cos();
                        self.part(
                            root,
                            cylinder(0.0, 0.1, 0.2),
                            &skin,
                            [sn * 0.55, 0.08 - len - 0.04, cs * 0.44],
                            [180.0, 0.0, 0.0],
                            ONE,
                        );
                    }
                }
            }
            "wrap" => {
                // cloth wound round a forearm
                for i in 0..3 {
                    self.part(
                        root,
                        torus(0.42, 0.56),
                        &skin,
                        [0.0, i as f32 * 0.32 - 0.32, 0.0],
                        [0.0, 0.0, if i % 2 == 0 { 8.0 } else { -8.0 }],
                        [1.0, 2.2, 1.0],
                    );
                }
            }
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
        if self.shape_is("sling") {
            self.sling(root);
            return;
        }
        if self.shape_is("crossbow") {
            self.crossbow(root);
            return;
        }
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

/// The head and the metal of a weapon's shape: "spear", "spear:silver".
fn shape_parts(shape: Option<&str>, default: &str) -> (String, String) {
    let s = shape.unwrap_or(default);
    match s.split_once(':') {
        Some((form, metal)) => (form.to_string(), metal.to_string()),
        None => (s.to_string(), "steel".to_string()),
    }
}

impl Kit<'_> {
    /// A weapon's metal: steel, silver, crude dark iron, or runed steel
    /// with a faint glow.
    fn weapon_metal(&mut self, metal: &str) -> Gd<Material> {
        let (name, fallback) = match metal {
            "silver" => ("silver", Color::from_rgb(0.86, 0.88, 0.92)),
            "crude" => ("dark_iron", Color::from_rgb(0.3, 0.28, 0.26)),
            "runed" => ("runed_steel", Color::from_rgb(0.7, 0.76, 0.84)),
            _ => ("steel", Color::from_rgb(0.72, 0.75, 0.8)),
        };
        self.plain(name, fallback)
    }

    /// A flat plate lying in the object's plane: centre (x, z), width and
    /// length, turned `yaw` degrees about y (0: along z).
    fn plate(
        &mut self,
        root: &mut Gd<Node3D>,
        mat: &Gd<Material>,
        at: [f32; 2],
        size: [f32; 2],
        yaw: f32,
    ) {
        self.part(
            root,
            cuboid(size[0], 0.012, size[1]),
            mat,
            [at[0], 0.03, at[1]],
            [0.0, yaw, 0.0],
            ONE,
        );
    }

    /// A flat point: its base centred at (x, z), `len` long towards `yaw`
    /// (0: +z, 90: +x).
    fn point(
        &mut self,
        root: &mut Gd<Node3D>,
        mat: &Gd<Material>,
        at: [f32; 2],
        width: f32,
        len: f32,
        yaw: f32,
    ) {
        let (s, c) = yaw.to_radians().sin_cos();
        let mid = [at[0] + s * len / 2.0, at[1] + c * len / 2.0];
        self.part(
            root,
            prism(width, len, 0.012),
            mat,
            [mid[0], 0.03, mid[1]],
            [90.0, yaw, 0.0],
            ONE,
        );
    }

    /// A round rod lying along z from `z0` to `z1` (at x).
    fn rod(
        &mut self,
        root: &mut Gd<Node3D>,
        mat: &Gd<Material>,
        x: f32,
        z: (f32, f32),
        r: (f32, f32),
    ) {
        self.part(
            root,
            cylinder(r.1, r.0, (z.1 - z.0).abs()),
            mat,
            [x, 0.03, (z.0 + z.1) / 2.0],
            [90.0, 0.0, 0.0],
            ONE,
        );
    }

    /// A curved blade: short plates along an arc of radius `r` about
    /// (x, z), from angle `a0` to `a1` (degrees; 0 is +z, 90 is +x).
    #[allow(clippy::too_many_arguments)]
    fn arc(
        &mut self,
        root: &mut Gd<Node3D>,
        mat: &Gd<Material>,
        centre: [f32; 2],
        r: f32,
        a0: f32,
        a1: f32,
        width: f32,
    ) {
        let n = 6;
        let seg = r * (a1 - a0).abs().to_radians() / n as f32 + 0.01;
        for i in 0..n {
            let a = a0 + (a1 - a0) * (i as f32 + 0.5) / n as f32;
            let (s, c) = a.to_radians().sin_cos();
            let w = width * (1.0 - 0.5 * i as f32 / n as f32);
            self.plate(
                root,
                mat,
                [centre[0] + s * r, centre[1] + c * r],
                [w, seg],
                a + 90.0,
            );
        }
    }

    /// A shafted weapon lying along z, its head at +z: spears, a trident, a
    /// lance, the polearms by the appearance's head, a staff bound in iron.
    fn pole(&mut self, root: &mut Gd<Node3D>) {
        let (form, metal) = shape_parts(self.shape.as_deref(), "spear");
        let wood = self.skin.clone();
        let steel = self.weapon_metal(&metal);
        let iron = self.plain("iron", Color::from_rgb(0.35, 0.33, 0.3));
        // a shaft about 3 cm across at the length it is drawn
        let thick = match form.as_str() {
            "staff" => 0.014,
            "stout" => 0.012,
            "javelin" => 0.008,
            _ => 0.01,
        };
        let top = match form.as_str() {
            "staff" => 0.5,
            "lance" => 0.05,
            _ => 0.34,
        };
        self.rod(root, &wood, 0.0, (-0.5, top), (thick, thick * 0.9));
        if form != "staff" && form != "lance" {
            self.rod(
                root,
                &iron,
                0.0,
                (top - 0.05, top + 0.01),
                (thick * 1.25, thick * 1.1),
            );
        }
        // a polearm's head, a little larger than life, so it reads at a
        // glance (and in an icon, where the whole shaft must fit)
        let big = !matches!(
            form.as_str(),
            "staff" | "lance" | "spear" | "stout" | "javelin" | "trident"
        );
        let k = if big { 1.45 } else { 1.0 };
        let mut head = Node3D::new_alloc();
        head.set_position(Vector3::new(0.0, 0.03 * (1.0 - k), top * (1.0 - k)));
        head.set_scale(Vector3::new(k, k, k));
        root.add_child(&head);
        let root = &mut head;
        let t = top;
        match form.as_str() {
            "staff" => {
                for z in [-0.5f32, -0.38, 0.38] {
                    self.rod(root, &iron, 0.0, (z, z + 0.05), (0.017, 0.017));
                }
                self.part(root, sphere(0.018), &iron, [0.0, 0.03, 0.5], FLAT, ONE);
            }
            "lance" => {
                self.rod(root, &steel, 0.0, (t, 0.5), (0.055, 0.004));
                self.part(
                    root,
                    cylinder(0.09, 0.07, 0.03),
                    &steel,
                    [0.0, 0.03, t],
                    [90.0, 0.0, 0.0],
                    ONE,
                );
                self.rod(root, &iron, 0.0, (-0.5, -0.44), (0.016, 0.016));
            }
            "trident" => {
                self.plate(root, &steel, [0.0, t + 0.02], [0.16, 0.025], 0.0);
                for x in [-0.07f32, 0.0, 0.07] {
                    let len = if x == 0.0 { 0.15 } else { 0.12 };
                    self.plate(root, &steel, [x, t + 0.02 + len / 2.0], [0.014, len], 0.0);
                    self.point(root, &steel, [x, t + 0.02 + len], 0.03, 0.04, 0.0);
                }
            }
            "fork" => {
                for x in [-0.035f32, 0.035] {
                    self.plate(root, &steel, [x, t + 0.08], [0.016, 0.16], 0.0);
                    self.point(root, &steel, [x, t + 0.16], 0.03, 0.05, 0.0);
                }
                self.plate(root, &steel, [0.0, t + 0.01], [0.1, 0.02], 0.0);
            }
            "broad" => {
                // a broad leaf with lugs at its base (vulgar polearm)
                self.point(root, &steel, [0.0, t + 0.02], 0.12, 0.2, 0.0);
                self.point(root, &steel, [0.0, t + 0.03], 0.12, 0.03, 180.0);
                for s in [-1.0f32, 1.0] {
                    self.point(root, &steel, [s * 0.05, t + 0.02], 0.03, 0.05, s * 50.0);
                }
            }
            "hilted" => {
                // a spike with a crossguard of two side prongs
                self.plate(root, &steel, [0.0, t + 0.015], [0.14, 0.022], 0.0);
                self.point(root, &steel, [0.0, t + 0.02], 0.04, 0.18, 0.0);
                for s in [-1.0f32, 1.0] {
                    self.point(root, &steel, [s * 0.06, t + 0.015], 0.025, 0.07, s * 35.0);
                }
            }
            "glaive" => {
                // one long edge, the back straight
                self.plate(root, &steel, [0.018, t + 0.09], [0.05, 0.18], 0.0);
                self.point(root, &steel, [0.018, t + 0.18], 0.05, 0.06, -12.0);
            }
            "halberd" => {
                self.plate(root, &steel, [0.075, t + 0.02], [0.11, 0.1], 0.0);
                self.plate(root, &steel, [0.13, t + 0.02], [0.03, 0.15], 0.0);
                self.point(root, &steel, [-0.01, t + 0.02], 0.035, 0.08, -90.0);
                self.point(root, &steel, [0.0, t + 0.06], 0.035, 0.14, 0.0);
            }
            "bardiche" => {
                self.plate(root, &steel, [0.055, t - 0.02], [0.07, 0.26], 0.0);
                self.arc(root, &steel, [0.0, t - 0.02], 0.09, 20.0, 160.0, 0.03);
                self.point(root, &steel, [0.055, t + 0.11], 0.07, 0.05, -20.0);
            }
            "cleaver" => {
                self.plate(root, &steel, [0.05, t + 0.07], [0.1, 0.16], 0.0);
                self.point(root, &steel, [0.05, t + 0.15], 0.1, 0.04, 0.0);
            }
            "sickle" => {
                self.plate(root, &steel, [0.0, t + 0.04], [0.02, 0.08], 0.0);
                self.arc(root, &steel, [0.08, t + 0.08], 0.08, -80.0, 80.0, 0.035);
            }
            "hook" => {
                self.plate(root, &steel, [0.015, t + 0.08], [0.035, 0.16], 0.0);
                self.arc(root, &steel, [-0.03, t + 0.17], 0.045, 90.0, 250.0, 0.025);
            }
            "billhook" => {
                self.point(root, &steel, [0.0, t + 0.02], 0.04, 0.18, 0.0);
                self.arc(root, &steel, [0.055, t + 0.05], 0.045, -60.0, 120.0, 0.025);
                self.point(root, &steel, [-0.015, t + 0.06], 0.03, 0.06, -90.0);
            }
            "pronged" => {
                // a hammer of prongs and a spike (lucern hammer)
                self.plate(root, &steel, [0.0, t + 0.03], [0.16, 0.04], 0.0);
                for x in [0.05f32, 0.08] {
                    self.point(root, &steel, [x, t + 0.05], 0.02, 0.04, 0.0);
                }
                self.point(root, &steel, [-0.08, t + 0.03], 0.035, 0.06, -90.0);
                self.point(root, &steel, [0.0, t + 0.05], 0.03, 0.12, 0.0);
            }
            "beaked" => {
                self.plate(root, &steel, [0.03, t + 0.03], [0.06, 0.05], 0.0);
                self.arc(root, &steel, [-0.04, t - 0.02], 0.07, 0.0, -80.0, 0.03);
                self.point(root, &steel, [0.0, t + 0.05], 0.03, 0.12, 0.0);
            }
            form => {
                // spears: a leaf point (a stout one broader, a javelin slim)
                let (w, len) = match form {
                    "stout" => (0.07, 0.15),
                    "javelin" => (0.036, 0.12),
                    _ => (0.05, 0.14),
                };
                self.point(root, &steel, [0.0, t + 0.035], w, len, 0.0);
                self.point(root, &steel, [0.0, t + 0.04], w, 0.035, 180.0);
            }
        }
    }

    /// A mace lying along z, its head at +z: a flanged mace, a spiked
    /// morning star, a flail's ball on a chain, a club of wood, a thonged
    /// club.
    fn mace(&mut self, root: &mut Gd<Node3D>) {
        let (form, metal) = shape_parts(self.shape.as_deref(), "mace");
        let wood = self.skin.clone();
        let steel = self.weapon_metal(&metal);
        let grip = self.leather();
        let y = 0.03;
        match form.as_str() {
            "club" | "aklys" => {
                self.rod(root, &wood, 0.0, (-0.5, 0.45), (0.035, 0.075));
                self.part(
                    root,
                    sphere(0.075),
                    &wood,
                    [0.0, y, 0.45],
                    FLAT,
                    [1.0, 1.0, 0.7],
                );
                for (x, z) in [(0.05f32, 0.2f32), (-0.055, 0.32), (0.04, 0.05)] {
                    self.part(root, sphere(0.022), &wood, [x, y + 0.01, z], FLAT, ONE);
                }
                self.rod(root, &grip, 0.0, (-0.5, -0.32), (0.04, 0.04));
                if form == "aklys" {
                    self.part(root, torus(0.06, 0.075), &grip, [0.0, y, -0.55], FLAT, ONE);
                }
            }
            "flail" => {
                self.rod(root, &wood, 0.0, (-0.5, 0.0), (0.028, 0.03));
                self.rod(root, &grip, 0.0, (-0.5, -0.3), (0.034, 0.034));
                self.part(root, sphere(0.036), &steel, [0.0, y, 0.01], FLAT, ONE);
                for i in 0..4 {
                    let z = 0.07 + i as f32 * 0.06;
                    let roll = if i % 2 == 0 { 0.0 } else { 90.0 };
                    self.part(
                        root,
                        torus(0.016, 0.026),
                        &steel,
                        [0.0, y, z],
                        [90.0, 0.0, roll],
                        [1.0, 1.0, 1.6],
                    );
                }
                self.spiked_ball(root, &steel, 0.38, 0.085);
            }
            "star" => {
                self.rod(root, &wood, 0.0, (-0.5, 0.3), (0.03, 0.034));
                self.rod(root, &grip, 0.0, (-0.5, -0.3), (0.036, 0.036));
                self.spiked_ball(root, &steel, 0.38, 0.095);
            }
            _ => {
                // a flanged head, heavy, on an iron haft
                self.rod(root, &steel, 0.0, (-0.5, 0.26), (0.024, 0.026));
                self.rod(root, &grip, 0.0, (-0.5, -0.28), (0.032, 0.032));
                self.rod(root, &steel, 0.0, (0.26, 0.46), (0.042, 0.036));
                for i in 0..6 {
                    let a = i as f32 * 60.0;
                    let (s, c) = a.to_radians().sin_cos();
                    self.part(
                        root,
                        prism(0.075, 0.16, 0.014),
                        &steel,
                        [s * 0.05, y + c * 0.05, 0.37],
                        [90.0, 0.0, -a],
                        [1.0, 1.0, 1.0],
                    );
                }
                self.part(root, sphere(0.026), &steel, [0.0, y, 0.47], FLAT, ONE);
            }
        }
    }

    /// A ball with spikes all round it, centred at z.
    fn spiked_ball(&mut self, root: &mut Gd<Node3D>, mat: &Gd<Material>, z: f32, r: f32) {
        let y = 0.03;
        self.part(root, sphere(r), mat, [0.0, y, z], FLAT, ONE);
        for (pitch, n) in [(-50.0f32, 5), (0.0, 7), (50.0, 5)] {
            for i in 0..n {
                let yaw = i as f32 * 360.0 / n as f32 + pitch;
                let (sp, cp) = pitch.to_radians().sin_cos();
                let (sy, cy) = yaw.to_radians().sin_cos();
                let dir = Vector3::new(cp * sy, sp, cp * cy);
                let at = Vector3::new(0.0, y, z) + dir * (r + 0.02);
                // a cone standing along `dir`
                let rot =
                    Basis::from_euler(EulerOrder::YXZ, Vector3::new(0.0, yaw.to_radians(), 0.0))
                        * Basis::from_euler(
                            EulerOrder::YXZ,
                            Vector3::new((90.0 - pitch).to_radians(), 0.0, 0.0),
                        );
                let e = rot.get_euler_with(EulerOrder::YXZ);
                self.part(
                    root,
                    cylinder(0.0, 0.016, 0.05),
                    mat,
                    [at.x, at.y, at.z],
                    [e.x.to_degrees(), e.y.to_degrees(), e.z.to_degrees()],
                    ONE,
                );
            }
        }
    }
}

impl Kit<'_> {
    /// A sword lying along z, its point at +z: a long, broad, great or
    /// short straight blade with a fuller, or a curved sabre or katana.
    fn sword(&mut self, root: &mut Gd<Node3D>) {
        let (form, metal) = shape_parts(self.shape.as_deref(), "long");
        let steel = self.weapon_metal(&metal);
        let dark = self.plain("dark_iron", Color::from_rgb(0.3, 0.28, 0.26));
        let grip = self.leather();
        let (w, grip_len) = match form.as_str() {
            "broad" => (0.11, 0.17),
            "great" => (0.085, 0.26),
            "short" => (0.085, 0.16),
            "katana" => (0.06, 0.24),
            "curved" => (0.07, 0.17),
            _ => (0.08, 0.18),
        };
        let z0 = -0.5 + grip_len + 0.05;
        let tip = 0.12;
        let len = 0.5 - tip - z0;
        self.rod(root, &grip, 0.0, (-0.49, z0 - 0.02), (0.02, 0.022));
        self.part(
            root,
            sphere(0.03),
            &steel,
            [0.0, 0.03, -0.49],
            FLAT,
            [1.0, 0.8, 1.0],
        );
        if form == "katana" {
            self.part(
                root,
                cylinder(0.06, 0.06, 0.02),
                &dark,
                [0.0, 0.03, z0 - 0.01],
                [90.0, 0.0, 0.0],
                ONE,
            );
        } else {
            let guard = if form == "broad" { 0.3 } else { 0.24 };
            self.plate(root, &steel, [0.0, z0 - 0.015], [guard, 0.03], 0.0);
        }
        match form.as_str() {
            "curved" | "katana" => {
                // a gentle curve: the edge sweeps back towards -x
                let r = 1.3f32;
                let sweep = (len / r).to_degrees();
                self.arc(root, &steel, [-r, z0], r, 90.0, 90.0 - sweep, w);
                let a = (90.0 - sweep).to_radians();
                let end = [-r + r * a.sin(), z0 + r * a.cos()];
                self.point(root, &steel, end, w * 0.55, tip, -sweep);
            }
            _ => {
                self.plate(root, &steel, [0.0, z0 + len / 2.0], [w, len], 0.0);
                self.point(root, &steel, [0.0, z0 + len], w, tip, 0.0);
                // the fuller: a dark groove down the middle
                self.part(
                    root,
                    cuboid(w * 0.22, 0.004, len * 0.8),
                    &dark,
                    [0.0, 0.037, z0 + len * 0.45],
                    FLAT,
                    ONE,
                );
            }
        }
    }

    /// A ration: a block wrapped in waxed paper and tied with twine.
    fn ration(&mut self, root: &mut Gd<Node3D>) {
        let wrap = self.skin.clone();
        let twine = self.plain("cloth", Color::from_rgb(0.4, 0.3, 0.2));
        self.part(
            root,
            cuboid(0.9, 0.3, 0.6),
            &wrap,
            [0.0, 0.15, 0.0],
            FLAT,
            ONE,
        );
        self.part(
            root,
            cuboid(0.92, 0.02, 0.05),
            &twine,
            [0.0, 0.305, 0.0],
            FLAT,
            ONE,
        );
        self.part(
            root,
            cuboid(0.05, 0.02, 0.62),
            &twine,
            [0.0, 0.305, 0.0],
            FLAT,
            ONE,
        );
        self.part(
            root,
            cuboid(0.93, 0.31, 0.05),
            &twine,
            [0.0, 0.15, 0.0],
            FLAT,
            ONE,
        );
        self.part(
            root,
            cuboid(0.05, 0.31, 0.63),
            &twine,
            [0.0, 0.15, 0.0],
            FLAT,
            ONE,
        );
        self.part(
            root,
            torus(0.03, 0.06),
            &twine,
            [0.0, 0.32, 0.0],
            FLAT,
            [1.4, 1.0, 1.0],
        );
    }

    /// A sling: two cords from a leather pouch, a loop at one end.
    fn sling(&mut self, root: &mut Gd<Node3D>) {
        let cord = self.plain("cloth", Color::from_rgb(0.45, 0.35, 0.22));
        let pouch = self.leather();
        self.part(
            root,
            sphere(0.12),
            &pouch,
            [0.0, 0.04, 0.0],
            FLAT,
            [1.0, 0.35, 1.4],
        );
        for (x0, z0, x1, z1) in [
            (-0.06f32, -0.14f32, -0.02f32, -0.5f32),
            (0.06, -0.14, 0.02, -0.5),
            (-0.06, 0.14, -0.1, 0.48),
            (0.06, 0.14, -0.06, 0.48),
        ] {
            let len = ((x1 - x0).powi(2) + (z1 - z0).powi(2)).sqrt();
            let yaw = (x1 - x0).atan2(z1 - z0).to_degrees();
            self.part(
                root,
                cylinder(0.008, 0.008, len),
                &cord,
                [(x0 + x1) / 2.0, 0.03, (z0 + z1) / 2.0],
                [90.0, yaw, 0.0],
                ONE,
            );
        }
        self.part(
            root,
            torus(0.03, 0.045),
            &cord,
            [0.0, 0.03, -0.52],
            FLAT,
            ONE,
        );
    }

    /// A crossbow: a wooden stock along z, the bow across it at the front,
    /// its string drawn back to the nut.
    fn crossbow(&mut self, root: &mut Gd<Node3D>) {
        let wood = self.skin.clone();
        let steel = self.weapon_metal("steel");
        let string = self.bone();
        self.plate(root, &wood, [0.0, -0.05], [0.08, 0.9], 0.0);
        self.part(
            root,
            cuboid(0.1, 0.06, 0.26),
            &wood,
            [0.0, 0.03, -0.38],
            FLAT,
            ONE,
        );
        for s in [-1.0f32, 1.0] {
            self.part(
                root,
                cylinder(0.014, 0.022, 0.4),
                &steel,
                [s * 0.19, 0.04, 0.34],
                [90.0, s * 72.0, 0.0],
                ONE,
            );
            // the string from the bow's tip back to the nut
            let (x0, z0, x1, z1) = (s * 0.37, 0.28f32, s * 0.03, 0.05f32);
            let len = ((x1 - x0).powi(2) + (z1 - z0).powi(2)).sqrt();
            let yaw = (x1 - x0).atan2(z1 - z0).to_degrees();
            self.part(
                root,
                cylinder(0.005, 0.005, len),
                &string,
                [(x0 + x1) / 2.0, 0.045, (z0 + z1) / 2.0],
                [90.0, yaw, 0.0],
                ONE,
            );
        }
        self.part(
            root,
            cuboid(0.03, 0.08, 0.04),
            &steel,
            [0.0, -0.01, -0.12],
            FLAT,
            ONE,
        );
    }
}
