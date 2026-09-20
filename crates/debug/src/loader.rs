//! Staged universe loader (v0.3.2 `settings-seed-loader`): a universe
//! load is a fixed ordered run of [`LoadStep`]s executed one per
//! frame, so the UI stays alive and the modal progress bar reports
//! honest progress. Pure + headless-testable — the `game_debug`
//! binary executes the steps (CPU regen + GPU re-upload) and draws
//! the overlay; this module only orders, labels, and fractions them.
//!
//! True async generation (worker threads) stays an M2 descent
//! streaming concern; the staged synchronous run is the loader for
//! this milestone.

/// What kicked off the load (drives the completion notice).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadSource {
    /// Settings Load button, Enter-on-field, or `R` re-roll.
    Panel,
    /// `--seed N` boot flag (runs on the first windowed frames).
    Boot,
}

/// One staged step of a universe load, in execution order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadStep {
    Galaxy,
    Journey,
    System,
    Cosmic,
    UploadMap,
    UploadSystem,
    UploadCosmic,
    Finalize,
}

impl LoadStep {
    /// Every step in execution order.
    pub const ALL: [LoadStep; 8] = [
        LoadStep::Galaxy,
        LoadStep::Journey,
        LoadStep::System,
        LoadStep::Cosmic,
        LoadStep::UploadMap,
        LoadStep::UploadSystem,
        LoadStep::UploadCosmic,
        LoadStep::Finalize,
    ];

    /// Modal step label (plain ASCII — the vendored UI font only
    /// guarantees the glyphs the shell already uses).
    pub const fn label(self) -> &'static str {
        match self {
            LoadStep::Galaxy => "Generating galaxy",
            LoadStep::Journey => "Charting journey",
            LoadStep::System => "Loading solar system",
            LoadStep::Cosmic => "Refreshing cosmic web",
            LoadStep::UploadMap => "Uploading galaxy",
            LoadStep::UploadSystem => "Uploading system",
            LoadStep::UploadCosmic => "Uploading cosmic web",
            LoadStep::Finalize => "Revealing universe",
        }
    }
}

/// In-flight load: the target seed, its source, and how many steps
/// have run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadPlan {
    pub seed: u64,
    pub source: LoadSource,
    done: usize,
}

impl LoadPlan {
    pub fn new(seed: u64, source: LoadSource) -> Self {
        LoadPlan {
            seed,
            source,
            done: 0,
        }
    }

    /// Step count (the modal denominator).
    pub const fn total() -> usize {
        LoadStep::ALL.len()
    }

    /// Steps completed so far.
    pub const fn done(&self) -> usize {
        self.done
    }

    /// Next step to execute, or `None` once every step has run.
    pub fn advance(&mut self) -> Option<LoadStep> {
        let step = LoadStep::ALL.get(self.done).copied()?;
        self.done += 1;
        Some(step)
    }

    /// Whether every step has run.
    pub const fn is_done(&self) -> bool {
        self.done >= LoadStep::ALL.len()
    }

    /// Determinate progress 0.0–1.0 (steps completed / total).
    pub fn progress(&self) -> f32 {
        self.done as f32 / LoadStep::ALL.len() as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_run_in_fixed_order() {
        let mut plan = LoadPlan::new(7, LoadSource::Panel);
        let mut ran = Vec::new();
        while let Some(step) = plan.advance() {
            ran.push(step);
        }
        assert_eq!(ran, LoadStep::ALL);
        assert!(plan.is_done());
    }

    #[test]
    fn progress_is_steps_over_total() {
        let mut plan = LoadPlan::new(7, LoadSource::Panel);
        assert_eq!(plan.progress(), 0.0);
        for (i, _) in LoadStep::ALL.iter().enumerate() {
            plan.advance();
            assert_eq!(plan.progress(), (i + 1) as f32 / LoadStep::ALL.len() as f32);
        }
        assert_eq!(plan.progress(), 1.0);
    }

    #[test]
    fn advance_past_end_returns_none() {
        let mut plan = LoadPlan::new(7, LoadSource::Boot);
        for _ in 0..LoadPlan::total() {
            assert!(plan.advance().is_some());
        }
        assert!(plan.advance().is_none());
        assert!(plan.advance().is_none());
        assert!(plan.is_done());
        assert_eq!(plan.done(), LoadPlan::total());
    }

    #[test]
    fn source_and_seed_ride_the_plan() {
        let plan = LoadPlan::new(99, LoadSource::Boot);
        assert_eq!(plan.seed, 99);
        assert_eq!(plan.source, LoadSource::Boot);
    }

    #[test]
    fn labels_are_nonempty_ascii() {
        for step in LoadStep::ALL {
            assert!(!step.label().is_empty());
            assert!(step.label().is_ascii(), "label must survive the UI font");
        }
    }
}
