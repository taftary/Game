//! Inputs-panel validation: pure parse + range checks for the
//! subdivisions and radius fields. The windowed binary calls these before
//! every regeneration; `HexSphere::generate` (which panics on invalid
//! radius) is only ever reached with a [`ValidParams`].

/// Slider/field range: notion clamps subdivisions to 0–8.
pub const MIN_SUBDIVISIONS: u32 = 0;
/// Slider/field range: notion clamps subdivisions to 0–8.
pub const MAX_SUBDIVISIONS: u32 = 8;
/// Warning styling kicks in above this (default N=6).
pub const WARN_SUBDIVISIONS: u32 = 6;

/// Live cost hint: resulting cell count for a subdivision level.
pub fn cell_count_hint(subdivisions: u32) -> usize {
    10 * 4usize.pow(subdivisions) + 2
}

/// True when the level needs the ">N=6 cost" warning styling.
pub fn subdiv_warning(subdivisions: u32) -> bool {
    subdivisions > WARN_SUBDIVISIONS
}

/// Subdivision levels validated against the 0–8 range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubdivError {
    /// Empty input.
    Empty,
    /// Not an integer.
    NotInteger,
    /// Integer outside 0–8.
    OutOfRange,
}

impl SubdivError {
    /// Inline hint shown under the field.
    pub fn hint(self) -> &'static str {
        match self {
            SubdivError::Empty => "enter subdivisions 0–8",
            SubdivError::NotInteger => "not an integer (0–8)",
            SubdivError::OutOfRange => "out of range (0–8)",
        }
    }
}

/// Radius values validated `> 0` and finite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadiusError {
    /// Empty input.
    Empty,
    /// Not a number.
    NotANumber,
    /// `<= 0` (would panic the generator).
    NotPositive,
    /// NaN or infinite (would panic the generator).
    NotFinite,
}

impl RadiusError {
    /// Inline hint shown under the field.
    pub fn hint(self) -> &'static str {
        match self {
            RadiusError::Empty => "enter a radius > 0",
            RadiusError::NotANumber => "not a number",
            RadiusError::NotPositive => "must be > 0",
            RadiusError::NotFinite => "must be finite",
        }
    }
}

/// Validated parameters: safe to pass to `HexSphere::generate`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ValidParams {
    pub subdivisions: u32,
    pub radius: f32,
}

/// Parse + range-check a subdivisions field value.
pub fn parse_subdivisions(text: &str) -> Result<u32, SubdivError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(SubdivError::Empty);
    }
    let value: u32 = trimmed.parse().map_err(|_| SubdivError::NotInteger)?;
    // `MIN_SUBDIVISIONS` is 0 = `u32::MIN`, so only the upper bound can fail.
    if value > MAX_SUBDIVISIONS {
        return Err(SubdivError::OutOfRange);
    }
    Ok(value)
}

/// Parse + validate a radius field value (`> 0`, finite).
pub fn parse_radius(text: &str) -> Result<f32, RadiusError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(RadiusError::Empty);
    }
    let value: f32 = trimmed.parse().map_err(|_| RadiusError::NotANumber)?;
    if !value.is_finite() {
        return Err(RadiusError::NotFinite);
    }
    if value <= 0.0 {
        return Err(RadiusError::NotPositive);
    }
    Ok(value)
}

/// Validation failure for [`validate`]: which field rejected the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamsError {
    Subdiv(SubdivError),
    Radius(RadiusError),
}

/// Validate both fields at once; `Ok` means Regenerate is enabled.
pub fn validate(subdivisions: &str, radius: &str) -> Result<ValidParams, ParamsError> {
    Ok(ValidParams {
        subdivisions: parse_subdivisions(subdivisions).map_err(ParamsError::Subdiv)?,
        radius: parse_radius(radius).map_err(ParamsError::Radius)?,
    })
}

/// Regenerate is enabled only while both fields are valid.
pub fn can_regenerate(subdivisions: &str, radius: &str) -> bool {
    validate(subdivisions, radius).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subdivisions_accept_range_edges() {
        assert_eq!(parse_subdivisions("0"), Ok(0));
        assert_eq!(parse_subdivisions("6"), Ok(6));
        assert_eq!(parse_subdivisions("8"), Ok(8));
        assert_eq!(parse_subdivisions("  4  "), Ok(4));
    }

    #[test]
    fn subdivisions_reject_invalid() {
        assert_eq!(parse_subdivisions(""), Err(SubdivError::Empty));
        assert_eq!(parse_subdivisions("   "), Err(SubdivError::Empty));
        assert_eq!(parse_subdivisions("abc"), Err(SubdivError::NotInteger));
        assert_eq!(parse_subdivisions("6.5"), Err(SubdivError::NotInteger));
        assert_eq!(parse_subdivisions("-1"), Err(SubdivError::NotInteger));
        assert_eq!(parse_subdivisions("9"), Err(SubdivError::OutOfRange));
        assert_eq!(parse_subdivisions("100"), Err(SubdivError::OutOfRange));
    }

    #[test]
    fn radius_accepts_positive_finite() {
        assert_eq!(parse_radius("1.0"), Ok(1.0));
        assert_eq!(parse_radius("0.001"), Ok(0.001));
        assert_eq!(parse_radius(" 2 "), Ok(2.0));
        assert_eq!(parse_radius("1e3"), Ok(1000.0));
    }

    #[test]
    fn radius_rejects_generator_panics() {
        assert_eq!(parse_radius(""), Err(RadiusError::Empty));
        assert_eq!(parse_radius("abc"), Err(RadiusError::NotANumber));
        assert_eq!(parse_radius("0"), Err(RadiusError::NotPositive));
        assert_eq!(parse_radius("0.0"), Err(RadiusError::NotPositive));
        assert_eq!(parse_radius("-1"), Err(RadiusError::NotPositive));
        assert_eq!(parse_radius("NaN"), Err(RadiusError::NotFinite));
        assert_eq!(parse_radius("inf"), Err(RadiusError::NotFinite));
        assert_eq!(parse_radius("-inf"), Err(RadiusError::NotFinite));
    }

    #[test]
    fn cost_hint_matches_mesh_formula() {
        let expected = [12, 42, 162, 642, 2562, 10242, 40962, 163842, 655362];
        for (n, cells) in expected.iter().enumerate() {
            assert_eq!(cell_count_hint(n as u32), *cells, "N={n}");
        }
    }

    #[test]
    fn warning_only_above_default() {
        assert!(!subdiv_warning(6));
        assert!(subdiv_warning(7));
        assert!(subdiv_warning(8));
    }

    #[test]
    fn validate_gates_regenerate() {
        assert!(can_regenerate("6", "1.0"));
        assert!(!can_regenerate("9", "1.0"));
        assert!(!can_regenerate("6", "0"));
        assert!(!can_regenerate("", ""));
        assert_eq!(
            validate("6", "1.0"),
            Ok(ValidParams {
                subdivisions: 6,
                radius: 1.0
            })
        );
        assert_eq!(
            validate("9", "1.0"),
            Err(ParamsError::Subdiv(SubdivError::OutOfRange))
        );
        assert_eq!(
            validate("6", "0"),
            Err(ParamsError::Radius(RadiusError::NotPositive))
        );
    }

    #[test]
    fn hints_are_nonempty() {
        for hint in [
            SubdivError::Empty.hint(),
            SubdivError::NotInteger.hint(),
            SubdivError::OutOfRange.hint(),
            RadiusError::Empty.hint(),
            RadiusError::NotANumber.hint(),
            RadiusError::NotPositive.hint(),
            RadiusError::NotFinite.hint(),
        ] {
            assert!(!hint.is_empty());
        }
    }
}
