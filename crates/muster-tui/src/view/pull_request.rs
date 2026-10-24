//! A pull request beside a worktree or a ticket: its number, a word for
//! where it stands and a mark for its checks, in its state's colours.

use super::theme::Theme;
use muster_core::{Checks, PullRequestState, PullRequestStatus, Review};
use ratatui::style::Style;
use ratatui::text::Span;

/// `#42 approved ✓`. While it is open the word is its review's, if one is
/// asked for or given, and the mark its checks'; `compact` leaves out the
/// word of an open one with a mark, for a narrow line.
pub(super) fn badge(status: &PullRequestStatus, compact: bool, theme: Theme) -> Vec<Span<'static>> {
    let number = format!("#{}", status.number);
    let (word, style) = match (status.state, status.review) {
        (PullRequestState::Merged, _) => ("merged", Style::default().fg(theme.merged)),
        (PullRequestState::Closed, _) => ("closed", Style::default().dim()),
        (PullRequestState::Draft, _) => ("draft", Style::default().dim()),
        (PullRequestState::Open, Some(Review::Approved)) => {
            ("approved", Style::default().fg(theme.done))
        }
        (PullRequestState::Open, Some(Review::ChangesRequested)) => {
            ("changes", Style::default().fg(theme.waiting))
        }
        (PullRequestState::Open, Some(Review::Required)) => {
            ("review", Style::default().fg(theme.working))
        }
        (PullRequestState::Open, None) => ("open", Style::default()),
    };
    let mark = status
        .checks
        .filter(|_| status.state.is_open())
        .map(|checks| match checks {
            Checks::Passing => Span::styled("✓", Style::default().fg(theme.done)),
            Checks::Failing => Span::styled("✗", Style::default().fg(theme.waiting)),
            Checks::Pending => Span::styled("◌", Style::default().fg(theme.working)),
        });
    let number_style = match status.state {
        PullRequestState::Merged => Style::default().fg(theme.merged),
        PullRequestState::Closed | PullRequestState::Draft => Style::default().dim(),
        PullRequestState::Open => Style::default(),
    };
    let mut spans = vec![Span::styled(number, number_style)];
    if !(compact && mark.is_some()) {
        spans.extend([Span::raw(" "), Span::styled(word, style)]);
    }
    if let Some(mark) = mark {
        spans.extend([Span::raw(" "), mark]);
    }
    spans
}

/// How many cells `spans` take.
pub(super) fn width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(Span::width).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(spans: &[Span<'_>]) -> String {
        spans.iter().map(|span| span.content.as_ref()).collect()
    }

    fn status(state: PullRequestState) -> PullRequestStatus {
        PullRequestStatus {
            number: 42,
            url: "https://github.com/o/app/pull/42".to_owned(),
            state,
            checks: Some(Checks::Failing),
            review: Some(Review::ChangesRequested),
        }
    }

    #[test]
    fn an_open_one_says_its_review_and_its_checks_and_a_closed_one_only_how_it_ended() {
        let open = status(PullRequestState::Open);
        assert_eq!(
            text(&badge(&open, false, crate::view::theme::plain())),
            "#42 changes ✗"
        );
        assert_eq!(
            text(&badge(&open, true, crate::view::theme::plain())),
            "#42 ✗"
        );
        assert_eq!(
            text(&badge(
                &status(PullRequestState::Draft),
                false,
                crate::view::theme::plain()
            )),
            "#42 draft ✗"
        );
        let merged = status(PullRequestState::Merged);
        assert_eq!(
            text(&badge(&merged, false, crate::view::theme::plain())),
            "#42 merged"
        );
        assert_eq!(
            text(&badge(&merged, true, crate::view::theme::plain())),
            "#42 merged",
            "nothing to leave out"
        );
        let mut plain = status(PullRequestState::Open);
        (plain.checks, plain.review) = (None, None);
        assert_eq!(
            text(&badge(&plain, true, crate::view::theme::plain())),
            "#42 open"
        );
        assert_eq!(width(&badge(&plain, true, crate::view::theme::plain())), 8);
    }
}
