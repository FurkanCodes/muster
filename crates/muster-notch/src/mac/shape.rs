//! The panel's shape and its motion, through Core Animation: the outline
//! `geometry` works out, as the mask of the panel's black, eased from rest
//! to open and back, each change starting from wherever the shape is on
//! screen, so a change of mind halfway never jumps; and the few things
//! that move, which Core Animation moves without waking the program. Under
//! the person's Reduce Motion setting the shape changes at once, the list
//! only fades, and nothing pulses or breathes.

use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWorkspace};
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{CGMutablePath, CGPath};
use objc2_foundation::{NSNumber, NSString};
use objc2_quartz_core::{
    CABasicAnimation, CACurrentMediaTime, CALayer, CAMediaTiming, CAMediaTimingFunction,
    CAShapeLayer, CATransaction, kCAFillModeBackwards,
};

use crate::geometry::{Rect, Segment};

/// How a change of shape moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Motion {
    /// Opening: a layout change, eased out.
    Open,
    /// Closing: quicker, as a way out is.
    Close,
    /// The list growing or shrinking while open.
    Resize,
}

impl Motion {
    /// How long it takes, in seconds.
    fn seconds(self) -> f64 {
        match self {
            Self::Open => 0.32,
            Self::Close => 0.24,
            Self::Resize => 0.2,
        }
    }
}

/// Whether the person asks for less motion.
pub(super) fn reduce_motion() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

/// An ease out that settles softly: quick to start, no overshoot.
fn ease_out() -> Retained<CAMediaTimingFunction> {
    CAMediaTimingFunction::functionWithControlPoints(0.22, 1.0, 0.36, 1.0)
}

/// `outline` as a path.
fn path(outline: &[Segment]) -> CFRetained<CGMutablePath> {
    let path = CGMutablePath::new();
    let at = Some(&*path);
    let none = std::ptr::null();
    for segment in outline {
        match *segment {
            Segment::Move(x, y) => {
                // SAFETY: CGPath's construction functions take a transform
                // or null; null is given, as nothing is transformed.
                unsafe { CGMutablePath::move_to_point(at, none, x, y) };
            }
            Segment::Line(x, y) => {
                // SAFETY: as above.
                unsafe { CGMutablePath::add_line_to_point(at, none, x, y) };
            }
            Segment::Quad((cx, cy), (x, y)) => {
                // SAFETY: as above.
                unsafe { CGMutablePath::add_quad_curve_to_point(at, none, cx, cy, x, y) };
            }
            Segment::Close => CGMutablePath::close_subpath(at),
        }
    }
    path
}

/// The panel's shape: the mask over its black.
pub(super) struct Shape {
    mask: Retained<CAShapeLayer>,
}

impl Shape {
    /// The mask of `view`'s layer.
    pub(super) fn new(view: &NSView) -> Self {
        let mask = CAShapeLayer::new();
        if let Some(layer) = view.layer() {
            // SAFETY: the mask is a layer of its own, held by `Shape` as
            // long as the view, and given to no other layer.
            unsafe { layer.setMask(Some(&mask)) };
        }
        Self { mask }
    }

    /// The mask fills a panel of `size`.
    pub(super) fn place(&self, size: Rect) {
        quietly(|| {
            self.mask
                .setFrame(super::ns_rect(Rect::new(0.0, 0.0, size.width, size.height)));
        });
    }

    /// The shape `outline`: moved to as `motion` says, from the shape on
    /// screen now, or at once.
    pub(super) fn set(&self, outline: &[Segment], motion: Option<Motion>) {
        let to = path(outline);
        let from = motion
            .filter(|_| !reduce_motion())
            .and_then(|_| self.on_screen());
        quietly(|| self.mask.setPath(Some(&to)));
        let (Some(motion), Some(from)) = (motion, from) else {
            self.mask.removeAllAnimations();
            return;
        };
        let change = CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str("path")));
        let (from, to): (&CGPath, &CGPath) = (&from, &to);
        // SAFETY: a path animation's values are paths, as these are.
        unsafe { change.setFromValue(Some(from.as_ref())) };
        // SAFETY: as above.
        unsafe { change.setToValue(Some(to.as_ref())) };
        change.setDuration(motion.seconds());
        change.setTimingFunction(Some(&ease_out()));
        self.mask
            .addAnimation_forKey(&change, Some(&NSString::from_str("shape")));
    }

    /// The shape as drawn this moment, partway through a change or not.
    fn on_screen(&self) -> Option<Retained<CGPath>> {
        // SAFETY: the presentation layer is only read, as Core Animation
        // asks of it.
        let shown = unsafe { self.mask.presentationLayer() }
            .and_then(|layer| layer.downcast::<CAShapeLayer>().ok());
        shown
            .and_then(|layer| layer.path())
            .or_else(|| self.mask.path())
    }
}

/// Runs `change` with Core Animation's own animations of it held back.
fn quietly(change: impl FnOnce()) {
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    change();
    CATransaction::commit();
}

const MOTION: &str = "motion";

/// Fades `view` in, `after` seconds from now, sliding down a little into
/// place: a line of the list appearing. Under Reduce Motion, only fades.
pub(super) fn appear(view: &NSView, after: f64) {
    view.setAlphaValue(1.0);
    let Some(layer) = view.layer() else {
        return;
    };
    let start = CACurrentMediaTime() + after;
    let fade = number_animation("opacity", 0.0, 1.0, 0.22, start);
    layer.addAnimation_forKey(&fade, Some(&NSString::from_str("appear")));
    if !reduce_motion() {
        let slide = number_animation("transform.translation.y", 5.0, 0.0, 0.26, start);
        layer.addAnimation_forKey(&slide, Some(&NSString::from_str("slide")));
    }
}

/// Fades `view` out quickly: the list going.
pub(super) fn disappear(view: &NSView) {
    view.setAlphaValue(0.0);
    if let Some(layer) = view.layer() {
        let fade = number_animation("opacity", 1.0, 0.0, 0.12, CACurrentMediaTime());
        layer.addAnimation_forKey(&fade, Some(&NSString::from_str("appear")));
    }
}

/// `key` going from `from` to `to` in `seconds`, eased out, from `start`
/// (in Core Animation's time), showing `from` until then.
fn number_animation(
    key: &str,
    from: f64,
    to: f64,
    seconds: f64,
    start: f64,
) -> Retained<CABasicAnimation> {
    let change = CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str(key)));
    let (from, to) = (NSNumber::new_f64(from), NSNumber::new_f64(to));
    // SAFETY: these animations' values are numbers, as these are.
    unsafe { change.setFromValue(Some(&from)) };
    // SAFETY: as above.
    unsafe { change.setToValue(Some(&to)) };
    change.setDuration(seconds);
    change.setBeginTime(start);
    // SAFETY: a fill mode Core Animation itself defines.
    change.setFillMode(unsafe { kCAFillModeBackwards });
    change.setTimingFunction(Some(&ease_out()));
    change
}

/// Fades `layer` down and back up `times` times, for a new question.
pub(super) fn pulse(layer: &CALayer, times: f32) {
    fade(layer, 0.35, 0.75, times);
}

/// Core Animation owns the endless breath. A change to Reduce Motion
/// removes one already running, rather than waiting for it to end.
pub(super) fn breathe(layer: &CALayer) {
    breathing(layer, reduce_motion());
}

fn breathing(layer: &CALayer, reduced: bool) {
    if reduced {
        still(layer);
    } else if !moving(layer) {
        fade(layer, 0.3, 1.2, f32::INFINITY);
    }
}

/// `layer` still: no pulse, no breath.
pub(super) fn still(layer: &CALayer) {
    layer.removeAnimationForKey(&NSString::from_str(MOTION));
}

/// Whether `layer` moves now.
pub(super) fn moving(layer: &CALayer) -> bool {
    // SAFETY: the animation found is only looked at, never changed, as
    // Core Animation asks of one it runs.
    unsafe { layer.animationForKey(&NSString::from_str(MOTION)) }.is_some()
}

fn fade(layer: &CALayer, down_to: f64, each: f64, times: f32) {
    if reduce_motion() {
        still(layer);
        return;
    }
    let fade = CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str("opacity")));
    let (from, to) = (NSNumber::new_f64(1.0), NSNumber::new_f64(down_to));
    // SAFETY: an opacity animation's values are numbers, as these are.
    unsafe { fade.setFromValue(Some(&from)) };
    // SAFETY: as above.
    unsafe { fade.setToValue(Some(&to)) };
    fade.setDuration(each);
    fade.setAutoreverses(true);
    fade.setRepeatCount(times);
    // Eased both ways, so the fade has no corners.
    fade.setTimingFunction(Some(&CAMediaTimingFunction::functionWithControlPoints(
        0.37, 0.0, 0.63, 1.0,
    )));
    layer.addAnimation_forKey(&fade, Some(&NSString::from_str(MOTION)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduce_motion_stops_a_breath_already_running_arc_13() {
        let layer = CALayer::new();
        // Install an animation directly so the assertion does not depend
        // on this machine's accessibility preference.
        let animation =
            CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str("opacity")));
        layer.addAnimation_forKey(&animation, Some(&NSString::from_str(MOTION)));
        assert!(moving(&layer));
        breathing(&layer, true);
        assert!(!moving(&layer));
        breathing(&layer, true);
        assert!(!moving(&layer));
    }
}
