//! The panel's shape and its motion, through Core Animation: the outline
//! `geometry` works out, as the mask of the panel's black, sprung from
//! rest to open and back; and the few marks that move, which Core
//! Animation moves without waking the program. Under the person's Reduce
//! Motion setting nothing springs, pulses or breathes.

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWorkspace};
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{CGMutablePath, CGPath};
use objc2_foundation::NSString;
use objc2_quartz_core::{
    CABasicAnimation, CALayer, CAMediaTiming, CAShapeLayer, CASpringAnimation, CATransaction,
};

use crate::geometry::{Rect, Segment};

/// How long the panel takes to open or close, as the person sees it.
const SPRING: f64 = 0.35;

/// Whether the person asks for less motion.
pub(super) fn reduce_motion() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
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

    /// The shape `outline`, in a view of `size`: sprung from `from` where
    /// given (an outline of as many parts), else at once.
    pub(super) fn set(&self, size: Rect, outline: &[Segment], from: Option<&[Segment]>) {
        let to = path(outline);
        quietly(|| {
            self.mask
                .setFrame(super::ns_rect(Rect::new(0.0, 0.0, size.width, size.height)));
            self.mask.setPath(Some(&to));
        });
        let Some(from) = from.filter(|_| !reduce_motion()) else {
            self.mask.removeAllAnimations();
            return;
        };
        let from = path(from);
        let spring = CASpringAnimation::initWithPerceptualDuration_bounce(
            CASpringAnimation::alloc(),
            SPRING,
            0.0,
        );
        spring.setKeyPath(Some(&NSString::from_str("path")));
        let (from, to): (&CGPath, &CGPath) = (&from, &to);
        // SAFETY: a path animation's values are paths, as these are.
        unsafe { spring.setFromValue(Some(from.as_ref())) };
        // SAFETY: as above.
        unsafe { spring.setToValue(Some(to.as_ref())) };
        spring.setDuration(spring.settlingDuration());
        self.mask
            .addAnimation_forKey(&spring, Some(&NSString::from_str("shape")));
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

/// Fades `layer` down and back up `times` times, for a new question.
pub(super) fn pulse(layer: &CALayer, times: f32) {
    fade(layer, 0.35, 0.75, times);
}

/// Fades `layer` down and back up, gently and for as long as it is shown:
/// what works.
pub(super) fn breathe(layer: &CALayer) {
    fade(layer, 0.3, 1.2, f32::INFINITY);
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
    let (from, to) = (
        objc2_foundation::NSNumber::new_f64(1.0),
        objc2_foundation::NSNumber::new_f64(down_to),
    );
    // SAFETY: an opacity animation's values are numbers, as these are.
    unsafe { fade.setFromValue(Some(&from)) };
    // SAFETY: as above.
    unsafe { fade.setToValue(Some(&to)) };
    fade.setDuration(each);
    fade.setAutoreverses(true);
    fade.setRepeatCount(times);
    layer.addAnimation_forKey(&fade, Some(&NSString::from_str(MOTION)));
}
