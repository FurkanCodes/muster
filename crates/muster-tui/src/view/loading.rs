//! The one wait, as the desktop app draws it (its `Loading`): a rule across
//! its place with its words in the middle, and a light running along it
//! that lights the words as it passes behind them. Drawn at the motion's
//! step (`motion`); still with motion off. Every wait on a screen is one.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme::Theme;
use crate::app::App;

/// Steps the light takes from before the rule's start to past its end:
/// 1.6 s, as in the app.
const CYCLE: u64 = 16;

/// `text` in the middle of a rule `width` cells wide, for `app`.
pub(crate) fn rule(app: &App, text: &str, width: usize) -> Line<'static> {
    Line::from(spans(
        text,
        width,
        app.motion_step(),
        &super::theme::of(app),
    ))
}

/// The rule's spans: `text` between two runs of `─`, the light where
/// `step` puts it; none where it would not fit, only the words.
pub(crate) fn spans(
    text: &str,
    width: usize,
    step: Option<u64>,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let words = format!(" {text} ");
    let room = width.saturating_sub(words.chars().count());
    if room < 4 {
        return vec![Span::styled(text.to_owned(), theme.quiet)];
    }
    let before = room / 2;
    let after = room - before;
    // Where the light's middle is, in quarter cells from the rule's start
    // (so it moves smoothly along a narrow rule): from before its start to
    // past its end. It glows a tenth of the rule either side.
    let scaled = i64::try_from(width * 4).unwrap_or(i64::MAX);
    let reach = (scaled / 5).max(8);
    let light = step.map(|step| {
        let cycle = i64::try_from(CYCLE).unwrap_or(1);
        let at = i64::try_from(step % CYCLE).unwrap_or(0);
        at * (scaled + reach * 2) / cycle - reach
    });
    let line = Style::default().fg(theme.line);
    let near = theme.quiet;
    let lit = Style::default().fg(theme.text);
    let look = |cell: usize| {
        let Some(light) = light else {
            return line;
        };
        let distance = (i64::try_from(cell * 4 + 2).unwrap_or(i64::MAX) - light).abs();
        if distance * 4 <= reach {
            lit
        } else if distance * 2 <= reach {
            near
        } else {
            line
        }
    };
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut push = |text: String, style: Style| match spans.last_mut() {
        Some(last) if last.style == style => last.content.to_mut().push_str(&text),
        _ => spans.push(Span::styled(text, style)),
    };
    for cell in 0..before {
        push("─".to_owned(), look(cell));
    }
    let words_lit = light.is_some_and(|light| {
        let start = i64::try_from(before * 4).unwrap_or(i64::MAX);
        let end = i64::try_from((width - after) * 4).unwrap_or(i64::MAX);
        light + reach / 4 >= start && light <= end + reach / 4
    });
    let words_style = if words_lit {
        lit.add_modifier(Modifier::BOLD)
    } else {
        theme.quiet
    };
    push(words, words_style);
    for cell in width - after..width {
        push("─".to_owned(), look(cell));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::new(
            muster_core::settings::Palette::Muster,
            &muster_core::colours::TerminalColours::default(),
        )
    }

    fn text(spans: &[Span<'_>]) -> String {
        spans.iter().map(|span| span.content.as_ref()).collect()
    }

    #[test]
    fn its_words_sit_in_the_middle_of_a_rule_as_wide_as_its_place() {
        let drawn = spans("Loading tickets", 31, None, &theme());
        assert_eq!(
            text(&drawn),
            format!("{0} Loading tickets {0}", "─".repeat(7))
        );
        assert_eq!(text(&drawn).chars().count(), 31);
        // Too narrow for a rule: only its words.
        assert_eq!(
            text(&spans("Loading tickets", 18, None, &theme())),
            "Loading tickets"
        );
    }

    #[test]
    fn still_it_is_all_quiet_and_moving_its_light_runs_through_the_words() {
        let theme = theme();
        let still = spans("Loading", 40, None, &theme);
        assert!(still.iter().all(|span| span.style.fg != Some(theme.text)));
        // Somewhere in its cycle the light is on the words, and somewhere not.
        let words_lit = |step| {
            spans("Loading", 40, Some(step), &theme)
                .iter()
                .any(|span| span.content.contains("Loading") && span.style.fg == Some(theme.text))
        };
        assert!((0..CYCLE).any(words_lit));
        assert!(!(0..CYCLE).all(words_lit));
        // It goes on along the rule after the words, on the same cycle.
        let lit_after = |step| {
            let drawn = spans("Loading", 40, Some(step), &theme);
            let after = drawn
                .iter()
                .skip_while(|span| !span.content.contains("Loading"))
                .skip(1);
            after
                .into_iter()
                .any(|span| span.style.fg == Some(theme.text))
        };
        assert!((0..CYCLE).any(lit_after));
        for step in 0..CYCLE {
            assert_eq!(
                text(&spans("Loading", 40, Some(step), &theme))
                    .chars()
                    .count(),
                40
            );
        }
    }
}
