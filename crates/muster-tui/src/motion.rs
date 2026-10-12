//! Motion: the few things that move, so what is moving stands out. A
//! running session's mark spins, and what needs the person pulses. The
//! runtime counts out steps (`STEP`) and the view draws the step it is
//! given, never reading the clock, so drawing stays a function of the app.
//! While nothing runs or waits nothing moves, and nothing is drawn for it.

use std::time::{Duration, Instant};

use ratatui::style::{Modifier, Style};

/// How long each step of the motion is shown.
pub(crate) const STEP: Duration = Duration::from_millis(100);

/// A running session's mark, one frame a step.
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Steps in each phase of the pulse: bright, plain, dim, plain.
const PULSE_PHASE: u64 = 4;

/// The spinner's frame at `step`.
pub(crate) fn spinner(step: u64) -> &'static str {
    SPINNER[usize::try_from(step % SPINNER.len() as u64).unwrap_or(0)]
}

/// `style` as it pulses at `step`: bold, then plain, then dim, then plain
/// again, so it breathes rather than blinks.
pub(crate) fn pulse(style: Style, step: u64) -> Style {
    let plain = style.remove_modifier(Modifier::BOLD | Modifier::DIM);
    match (step / PULSE_PHASE) % 4 {
        0 => plain.add_modifier(Modifier::BOLD),
        2 => plain.add_modifier(Modifier::DIM),
        _ => plain,
    }
}

/// Counts out the steps from when the UI started.
#[derive(Debug)]
pub(crate) struct Steps {
    start: Instant,
    drawn: Option<u64>,
}

impl Steps {
    pub(crate) fn new(start: Instant) -> Self {
        Self { start, drawn: None }
    }

    /// The step at `now`, if it is not the one last given.
    pub(crate) fn due(&mut self, now: Instant) -> Option<u64> {
        let elapsed = now.saturating_duration_since(self.start).as_millis() / STEP.as_millis();
        let step = u64::try_from(elapsed).unwrap_or(u64::MAX);
        if self.drawn == Some(step) {
            return None;
        }
        self.drawn = Some(step);
        Some(step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn the_spinner_goes_round_its_frames_one_a_step() {
        assert_eq!(spinner(0), "⠋");
        assert_eq!(spinner(1), "⠙");
        assert_eq!(spinner(9), "⠏");
        assert_eq!(spinner(10), "⠋");
        assert_eq!(spinner(u64::MAX), SPINNER[5]);
    }

    #[test]
    fn the_pulse_goes_bright_plain_dim_plain_and_keeps_the_colour() {
        let red = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
        let phases: Vec<Modifier> = (0..16)
            .step_by(4)
            .map(|step| pulse(red, step).add_modifier)
            .collect();
        assert_eq!(
            phases,
            [
                Modifier::BOLD,
                Modifier::empty(),
                Modifier::DIM,
                Modifier::empty()
            ]
        );
        assert!((0..16).all(|step| pulse(red, step).fg == Some(Color::Red)));
        // Each phase holds for its steps, and the cycle starts again.
        assert_eq!(pulse(red, 3), pulse(red, 0));
        assert_eq!(pulse(red, 16), pulse(red, 0));
    }

    #[test]
    fn a_step_is_given_once_as_it_comes() {
        let start = Instant::now();
        let mut steps = Steps::new(start);
        assert_eq!(steps.due(start), Some(0));
        assert_eq!(steps.due(start + Duration::from_millis(60)), None);
        assert_eq!(steps.due(start + Duration::from_millis(100)), Some(1));
        assert_eq!(steps.due(start + Duration::from_millis(199)), None);
        // Steps missed while nothing moved are skipped, not caught up.
        assert_eq!(steps.due(start + Duration::from_millis(1_250)), Some(12));
        assert_eq!(steps.due(start + Duration::from_millis(1_290)), None);
    }
}
