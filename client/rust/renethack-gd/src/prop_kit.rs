//! Objects drawn in code by their model's shape (`organic_props`) that
//! once borrowed another's model: a towel, a leash, a beartrap, a harp, a
//! cream pie, a banana... Each part has its material; the first is the
//! look's own (tinted with the glyph's colour, which is the object's).
//! The pieces a demon wears on its bones are drawn the same way.

use super::*;
use crate::organic::Organic;
use crate::organic::props::Part;

const ONE: [f32; 3] = [1.0; 3];
const FLAT: [f32; 3] = [0.0; 3];

/// What a part of an object is made of.
#[derive(Clone, Copy)]
enum Stuff {
    /// The look's own material and colour.
    Own,
    /// The look's colour, dark (a banana's ends).
    Dark,
    /// The look's colour, polished and lit from inside (royal jelly).
    Gem,
    Iron,
    /// A blade's bright steel.
    Steel,
    Brass,
    Rope,
    /// A drum's heads, pale stretched hide.
    Hide,
    Tin,
    Crust,
    Cherry,
    Chocolate,
    Paper,
    Butter,
    Syrup,
    Leaf,
    /// A stripe or a band: off-white cloth or paper.
    Pale,
    /// What a mimic shows: an odd sheen.
    Odd,
    /// A serpent's belly: the look's colour, pale.
    Belly,
}

/// The parts of the object of `shape` and what each is made of.
fn parts(shape: &str) -> &'static [(Part, Stuff)] {
    use Part::*;
    match shape {
        "blindfold" => &[(Blindfold, Stuff::Own)],
        "towel" => &[(Towel, Stuff::Own), (TowelStripes, Stuff::Pale)],
        "leash" => &[(Leash, Stuff::Own), (LeashClasp, Stuff::Brass)],
        "grappling hook" => &[(Hook, Stuff::Own), (HookRope, Stuff::Rope)],
        "land mine" => &[(Mine, Stuff::Own), (MinePlate, Stuff::Iron)],
        "beartrap" => &[(Beartrap, Stuff::Own)],
        "harp" => &[(Harp, Stuff::Own), (HarpStrings, Stuff::Brass)],
        "drum" => &[
            (Drum, Stuff::Own),
            (DrumHeads, Stuff::Hide),
            (DrumCords, Stuff::Rope),
        ],
        "cream pie" => &[
            (PieDish, Stuff::Tin),
            (PieCrust, Stuff::Crust),
            (PieCream, Stuff::Own),
            (PieCherry, Stuff::Cherry),
        ],
        "candy bar" => &[
            (CandyWrapper, Stuff::Own),
            (CandyBand, Stuff::Pale),
            (CandyBar, Stuff::Chocolate),
        ],
        "fortune cookie" => &[(Cookie, Stuff::Own), (CookieSlip, Stuff::Paper)],
        "pancake" => &[
            (Pancakes, Stuff::Own),
            (Butter, Stuff::Butter),
            (Syrup, Stuff::Syrup),
        ],
        "lembas wafer" => &[(Lembas, Stuff::Own), (LembasLeaf, Stuff::Leaf)],
        "royal jelly" => &[(Jelly, Stuff::Gem)],
        "banana" => &[(Banana, Stuff::Own), (BananaEnds, Stuff::Dark)],
        "kelp" => &[(Kelp, Stuff::Own)],
        "eucalyptus" => &[(Leaf, Stuff::Own)],
        "strange" => &[(Strange, Stuff::Odd)],
        "serpent tail" => &[(SerpentTail, Stuff::Own), (SerpentBelly, Stuff::Belly)],
        "bladed arm" => &[(Arm, Stuff::Own), (ArmBlade, Stuff::Steel)],
        "bat wings" => &[(BatWings, Stuff::Own), (BatWingBones, Stuff::Dark)],
        _ => &[],
    }
}

impl Kit<'_> {
    /// An object of `organic_props`, by its model's shape.
    pub(super) fn prop(&mut self, root: &mut Gd<Node3D>) {
        let shape = self.shape.clone().unwrap_or_default();
        for &(part, stuff) in parts(&shape) {
            let mat = self.stuff(stuff);
            self.part(
                root,
                MeshKey::Organic(Organic::Prop(part)),
                &mat,
                FLAT,
                FLAT,
                ONE,
            );
        }
    }

    fn stuff(&mut self, stuff: Stuff) -> Gd<Material> {
        let flat = |kit: &mut Self, r: f32, g: f32, b: f32, finish: Finish| {
            kit.art.flat(Color::from_rgb(r, g, b), finish)
        };
        match stuff {
            Stuff::Own => self.skin.clone(),
            Stuff::Dark => self.dark(),
            Stuff::Gem => {
                let tint = self.tint;
                self.art.flat(tint, Finish::Gem)
            }
            Stuff::Iron => self.named("iron", Color::from_rgb(0.42, 0.4, 0.38)),
            Stuff::Steel => self.named("bright_steel", Color::from_rgb(0.77, 0.78, 0.82)),
            Stuff::Brass => flat(self, 0.78, 0.6, 0.28, Finish::Glossy),
            Stuff::Rope => flat(self, 0.6, 0.5, 0.34, Finish::Matte),
            Stuff::Hide => flat(self, 0.86, 0.8, 0.66, Finish::Matte),
            Stuff::Tin => flat(self, 0.7, 0.7, 0.72, Finish::Glossy),
            Stuff::Crust => flat(self, 0.78, 0.55, 0.28, Finish::Matte),
            Stuff::Cherry => flat(self, 0.7, 0.05, 0.08, Finish::Glossy),
            Stuff::Chocolate => flat(self, 0.3, 0.16, 0.09, Finish::Matte),
            Stuff::Paper => flat(self, 0.95, 0.94, 0.9, Finish::Matte),
            Stuff::Butter => flat(self, 0.98, 0.9, 0.55, Finish::Glossy),
            Stuff::Syrup => flat(self, 0.45, 0.2, 0.05, Finish::Glossy),
            Stuff::Leaf => flat(self, 0.36, 0.55, 0.22, Finish::Glossy),
            Stuff::Pale => flat(self, 0.92, 0.9, 0.85, Finish::Matte),
            Stuff::Odd => flat(self, 0.27, 0.22, 0.32, Finish::Glossy),
            Stuff::Belly => {
                let pale = self.tint.lerp(Color::from_rgb(0.86, 0.78, 0.6), 0.6);
                self.art.flat(pale, Finish::Glossy)
            }
        }
    }
}
