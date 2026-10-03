//! The engine's printf formats: literal text and conversions, the way the
//! catalog stores them and P7 sends them.

/// What a conversion prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConvKind {
    /// `%s`
    Str,
    /// `%d %i %u %ld %x`...
    Int,
    /// `%c`
    Char,
    /// `%f %e %g`
    Float,
    /// `%p`, `%n`: never in a shown text
    Other,
}

/// One conversion of a format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conv {
    pub kind: ConvKind,
    /// A `*` width or precision: the conversion takes that many more
    /// (integer) arguments before its own.
    pub stars: usize,
    /// The printed width, when it pads: `%-45s`.
    pub width: Option<usize>,
    /// `%.20s`: at most that many characters.
    pub precision: Option<usize>,
    /// The conversion as written: "%-45.*s".
    pub spec: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Lit(String),
    Conv(Conv),
}

/// A format split into its literal runs (with `%%` as `%`) and its
/// conversions. Text after a `%` that is no conversion stays literal.
pub fn parse_format(fmt: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut rest = fmt;
    while let Some(p) = rest.find('%') {
        lit.push_str(&rest[..p]);
        let after = &rest[p + 1..];
        if let Some(stripped) = after.strip_prefix('%') {
            lit.push('%');
            rest = stripped;
            continue;
        }
        match conversion(after) {
            Some((conv, used)) => {
                if !lit.is_empty() {
                    out.push(Segment::Lit(std::mem::take(&mut lit)));
                }
                out.push(Segment::Conv(Conv {
                    spec: format!("%{}", &after[..used]),
                    ..conv
                }));
                rest = &after[used..];
            }
            None => {
                lit.push('%');
                rest = after;
            }
        }
    }
    lit.push_str(rest);
    if !lit.is_empty() {
        out.push(Segment::Lit(lit));
    }
    out
}

/// The conversion at the start of `s` (just after its `%`) and the bytes
/// it takes: flags, width, precision, length, conversion.
fn conversion(s: &str) -> Option<(Conv, usize)> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && b"-+ #0".contains(&b[i]) {
        i += 1;
    }
    let mut stars = 0;
    let mut width = None;
    if i < b.len() && b[i] == b'*' {
        stars += 1;
        i += 1;
    } else {
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i > start {
            width = s[start..i].parse().ok();
        }
    }
    let mut precision = None;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        if i < b.len() && b[i] == b'*' {
            stars += 1;
            i += 1;
        } else {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            precision = Some(s[start..i].parse().unwrap_or(0));
        }
    }
    for len in ["hh", "ll", "h", "l", "L", "z", "j", "t"] {
        if s[i..].starts_with(len) {
            i += len.len();
            break;
        }
    }
    let kind = match b.get(i)? {
        b's' => ConvKind::Str,
        b'd' | b'i' | b'o' | b'u' | b'x' | b'X' => ConvKind::Int,
        b'c' => ConvKind::Char,
        b'e' | b'E' | b'f' | b'g' | b'G' => ConvKind::Float,
        b'p' | b'n' => ConvKind::Other,
        _ => return None,
    };
    let conv = Conv {
        kind,
        stars,
        width,
        precision,
        spec: String::new(),
    };
    Some((conv, i + 1))
}

/// The conversions of a parsed format, in order.
pub fn convs(segments: &[Segment]) -> impl Iterator<Item = &Conv> {
    segments.iter().filter_map(|s| match s {
        Segment::Conv(c) => Some(c),
        Segment::Lit(_) => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(s: &str) -> Segment {
        Segment::Lit(s.into())
    }

    #[test]
    fn formats_split_into_text_and_conversions() {
        let segs = parse_format("You hit %s for %d%% (%ld)!");
        assert_eq!(segs.len(), 7);
        assert_eq!(segs[0], lit("You hit "));
        let Segment::Conv(c) = &segs[1] else {
            panic!("a conversion")
        };
        assert_eq!((c.kind, c.spec.as_str()), (ConvKind::Str, "%s"));
        assert_eq!(segs[2], lit(" for "));
        assert_eq!(segs[4], lit("% ("));
        let Segment::Conv(c) = &segs[5] else {
            panic!("a conversion")
        };
        assert_eq!((c.kind, c.spec.as_str()), (ConvKind::Int, "%ld"));
        assert_eq!(segs[6], lit(")!"));
    }

    #[test]
    fn widths_precisions_and_stars_are_kept() {
        let segs = parse_format("%c - %-45.*s%s");
        let cs: Vec<_> = convs(&segs).collect();
        assert_eq!(cs.len(), 3);
        assert_eq!(cs[0].kind, ConvKind::Char);
        assert_eq!(
            (cs[1].stars, cs[1].width, cs[1].spec.as_str()),
            (1, Some(45), "%-45.*s")
        );
        assert_eq!(cs[2].kind, ConvKind::Str);
        let segs = parse_format("%.20s and %6ld");
        let cs: Vec<_> = convs(&segs).collect();
        assert_eq!(cs[0].precision, Some(20));
        assert_eq!((cs[1].kind, cs[1].width), (ConvKind::Int, Some(6)));
    }

    #[test]
    fn a_stray_percent_stays_text() {
        assert_eq!(parse_format("50%"), vec![lit("50%")]);
        assert_eq!(parse_format("5%z"), vec![lit("5%z")]);
    }
}
