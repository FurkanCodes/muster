//! Motion: the few things that move, so what is moving stands out. A
//! running session's mark spins, what needs the person pulses, and a
//! session's line flashes as its status comes to want a look. The
//! runtime counts out steps (`STEP`) and the view draws the step it is
//! given, never reading the clock, so drawing stays a function of the app.
//! While nothing runs or waits nothing moves, and nothing is drawn for it.
//! Kep, at the top bar's start, blinks now and then while something works
//! and is glad a moment when a session finishes. A name cut short on the
//! selected line slides along, so it can be read whole.

use std::time::{Duration, Instant};

use ratatui::style::{Color, Modifier, Style};

/// How long each step of the motion is shown.
pub(crate) const STEP: Duration = Duration::from_millis(100);

/// A running session's mark, one frame a step.
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Steps in each phase of the pulse: bright, plain, dim, plain.
const PULSE_PHASE: u64 = 4;

/// How long a line flashes: lit for the first half, then in its colour
/// for the second, then settled.
pub(crate) const FLASH_STEPS: u64 = 10;

/// When a note under the screen dims, and when it goes, in steps: read
/// for four seconds, dim for one.
pub(crate) const NOTE_DIMS: u64 = 40;
pub(crate) const NOTE_GOES: u64 = 50;

/// Steps between Kep's blinks, and how long a blink lasts.
const BLINK_EVERY: u64 = 40;
const BLINK_STEPS: u64 = 2;

/// How long Kep is glad of a session that finished: three seconds.
pub(crate) const GLAD_STEPS: u64 = 30;

/// Steps a name cut short rests at its start before it slides, and at its
/// end before it starts again: a second and a half.
const SLIDE_REST: u64 = 15;

/// Steps a sliding name takes over each character: five a second, slow
/// enough to read as it goes.
const SLIDE_EVERY: u64 = 2;

/// How many characters a name of `len`, shown in `room`, has slid `steps`
/// after its line was selected: it rests, slides a character each
/// `SLIDE_EVERY` steps until its end is in view, rests there, and starts
/// again. A name that fits stays put.
pub(crate) fn slide(len: usize, room: usize, steps: u64) -> usize {
    let far = u64::try_from(len.saturating_sub(room)).unwrap_or(u64::MAX);
    if far == 0 {
        return 0;
    }
    let cycle = far
        .saturating_mul(SLIDE_EVERY)
        .saturating_add(2 * SLIDE_REST);
    let by = ((steps % cycle).saturating_sub(SLIDE_REST) / SLIDE_EVERY).min(far);
    usize::try_from(by).unwrap_or(0)
}

/// Whether Kep's eyes are shut for a blink at `step`.
pub(crate) fn blinking(step: u64) -> bool {
    step % BLINK_EVERY >= BLINK_EVERY - BLINK_STEPS
}

/// How a flashing line is drawn `age` steps into its flash, in `colour`:
/// lit (dark on the colour) for the first half, then in the colour, bold.
/// `None` once it has settled.
pub(crate) fn flash(colour: Color, on: Color, age: u64) -> Option<Flash> {
    match age {
        age if age < FLASH_STEPS / 2 => Some(Flash::Lit(
            Style::default()
                .fg(on)
                .bg(colour)
                .add_modifier(Modifier::BOLD),
        )),
        age if age < FLASH_STEPS => Some(Flash::Tinted(
            Style::default().fg(colour).add_modifier(Modifier::BOLD),
        )),
        _ => None,
    }
}

/// A flashing line's look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Flash {
    /// The whole line in this style, over its spans' own.
    Lit(Style),
    /// This style under the spans' own, so their marks keep their colours.
    Tinted(Style),
}

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

    /// When the step after the one at `now` begins.
    pub(crate) fn next(&self, now: Instant) -> Instant {
        let elapsed = now.saturating_duration_since(self.start).as_millis() / STEP.as_millis();
        let next = u32::try_from(elapsed + 1).unwrap_or(u32::MAX);
        self.start + STEP * next
    }
}

#[cfg(test)]
#[expect(clippy::disallowed_methods, reason = "tests time themselves")]
mod tests {
    use super::*;

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
    fn a_flash_is_lit_then_tinted_then_settles() {
        let lit = Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD);
        let tinted = Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD);
        assert_eq!(flash(Color::Green, Color::Black, 0), Some(Flash::Lit(lit)));
        assert_eq!(flash(Color::Green, Color::Black, 4), Some(Flash::Lit(lit)));
        assert_eq!(
            flash(Color::Green, Color::Black, 5),
            Some(Flash::Tinted(tinted))
        );
        assert_eq!(
            flash(Color::Green, Color::Black, 9),
            Some(Flash::Tinted(tinted))
        );
        assert_eq!(flash(Color::Green, Color::Black, 10), None);
    }

    #[test]
    fn a_cut_name_rests_slides_to_its_end_rests_and_starts_again() {
        // Twenty characters in sixteen: four to slide.
        let at = |steps| slide(20, 16, steps);
        assert_eq!(at(0), 0);
        assert_eq!(at(SLIDE_REST - 1), 0, "it rests first");
        assert_eq!(at(SLIDE_REST + 1), 0);
        assert_eq!(at(SLIDE_REST + 2), 1, "a character every two steps");
        assert_eq!(at(SLIDE_REST + 7), 3);
        assert_eq!(at(SLIDE_REST + 8), 4, "its end in view");
        assert_eq!(at(2 * SLIDE_REST + 7), 4, "it rests at its end");
        let cycle = 2 * SLIDE_REST + 8;
        assert_eq!(at(cycle), 0, "then starts again");
        assert_eq!(at(cycle + SLIDE_REST + 2), 1);
    }

    #[test]
    fn a_name_that_fits_never_slides() {
        assert!((0..100).all(|steps| slide(16, 16, steps) == 0));
        assert!((0..100).all(|steps| slide(3, 16, steps) == 0));
        assert_eq!(slide(usize::MAX, 0, u64::MAX), 0, "no overflow");
    }

    #[test]
    fn kep_blinks_for_two_steps_in_forty() {
        let shut: Vec<u64> = (0..120).filter(|step| blinking(*step)).collect();
        assert_eq!(shut, [38, 39, 78, 79, 118, 119]);
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
