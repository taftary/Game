//! Text validation contracts.

use planet_crafter_engine::text::{FontSize, TextError, TextRun};
use planet_crafter_tests::fixtures::sample_run;

#[test]
fn valid_run_exposes_content_and_size() {
    let run = sample_run();
    assert_eq!(run.text(), "hello");
    assert_eq!(run.size().points(), 16);
}

#[test]
fn zero_font_size_is_rejected() {
    assert_eq!(FontSize::new(0), Err(TextError::InvalidFontSize(0)));
}

#[test]
fn empty_text_is_rejected() {
    let size = FontSize::new(12).expect("12 is a valid size");
    assert_eq!(TextRun::new("", size), Err(TextError::EmptyText));
}

#[test]
fn text_errors_describe_the_failure() {
    assert_eq!(
        TextError::InvalidFontSize(0).to_string(),
        "invalid font size: 0"
    );
    assert_eq!(
        TextError::EmptyText.to_string(),
        "text content must not be empty"
    );
}
