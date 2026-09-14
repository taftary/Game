//! Resource construction validates input and reports errors explicitly.
//!
//! Demonstrates [error handling](../../book/practices/error-handling.md) and
//! [type-driven design](../../book/principles/type-driven-design.md): invalid
//! inputs become typed errors, never panics or silent defaults.

use planet_crafter_engine::text::{FontSize, TextError, TextRun};

fn main() -> Result<(), TextError> {
    let size = FontSize::new(16)?;
    let run = TextRun::new("validated", size)?;
    assert_eq!(run.text(), "validated");

    assert_eq!(FontSize::new(0), Err(TextError::InvalidFontSize(0)));
    println!("validated_resource: valid run accepted, invalid input rejected");
    Ok(())
}
