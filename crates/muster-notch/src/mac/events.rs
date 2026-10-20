//! AppKit owns mouse observation and a single one-shot timer. None polls
//! the pointer, and hidden panels keep neither (ARC-13, ARC-14).

use std::ptr::NonNull;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_app_kit::{
    NSApplicationDidChangeScreenParametersNotification, NSEvent, NSEventMask, NSWorkspace,
    NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification,
};
use objc2_foundation::{NSNotificationCenter, NSObjectProtocol, NSOperationQueue, NSTimer};

use super::SCREEN;

#[expect(
    clippy::disallowed_methods,
    reason = "ARC-03: the screen's one monotonic clock"
)]
pub(super) fn now() -> Instant {
    Instant::now()
}

pub(super) fn redraw() {
    SCREEN.with(|screen| {
        if let Some(screen) = screen.borrow_mut().as_mut() {
            screen.draw();
        }
    });
}

fn pointer() {
    SCREEN.with(|screen| {
        if let Some(screen) = screen.borrow_mut().as_mut() {
            screen.pointer_changed();
        }
    });
}

pub(super) struct Mouse {
    global: Option<Retained<AnyObject>>,
    local: Option<Retained<AnyObject>>,
}

impl Mouse {
    pub(super) fn new() -> Self {
        let mask = NSEventMask::MouseMoved
            | NSEventMask::LeftMouseDragged
            | NSEventMask::RightMouseDragged;
        let global = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            mask,
            &RcBlock::new(|_: NonNull<NSEvent>| pointer()),
        );
        // SAFETY: the local monitor observes only, returning the same valid
        // event pointer it was given. Both callbacks run on the main thread.
        let local = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                mask,
                &RcBlock::new(|event: NonNull<NSEvent>| {
                    pointer();
                    event.as_ptr()
                }),
            )
        };
        Self { global, local }
    }
}

impl Drop for Mouse {
    fn drop(&mut self) {
        for monitor in [&self.global, &self.local].into_iter().flatten() {
            // SAFETY: these are exactly the tokens NSEvent gave this owner.
            unsafe { NSEvent::removeMonitor(monitor) };
        }
    }
}

pub(super) struct Motion {
    center: Retained<NSNotificationCenter>,
    observer: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    screens: Retained<ProtocolObject<dyn NSObjectProtocol>>,
}

impl Motion {
    pub(super) fn new() -> Self {
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        // SAFETY: Apple's notification name, no object filter, delivery on
        // the main queue. The block captures nothing and is sendable.
        let observer = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
                None,
                Some(&NSOperationQueue::mainQueue()),
                &RcBlock::new(|_| redraw()),
            )
        };
        // SAFETY: Apple's screen-change notification, with main-queue
        // delivery and a sendable callback that captures nothing.
        let screens = unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSApplicationDidChangeScreenParametersNotification),
                None,
                Some(&NSOperationQueue::mainQueue()),
                &RcBlock::new(|_| redraw()),
            )
        };
        Self {
            center,
            observer,
            screens,
        }
    }
}

impl Drop for Motion {
    fn drop(&mut self) {
        // SAFETY: the token came from this notification center.
        unsafe { self.center.removeObserver((*self.observer).as_ref()) };
        // SAFETY: this token came from the default notification center.
        unsafe { NSNotificationCenter::defaultCenter().removeObserver((*self.screens).as_ref()) };
    }
}

#[derive(Default)]
pub(super) struct Timer {
    timer: Option<Retained<NSTimer>>,
    at: Option<u64>,
}

impl Timer {
    pub(super) fn set(&mut self, start: Instant, at: Option<u64>) {
        if self.at == at {
            return;
        }
        self.clear();
        self.at = at;
        if let Some(at) = at {
            let left = (start + Duration::from_millis(at.saturating_mul(100)))
                .saturating_duration_since(now());
            // SAFETY: made on the main thread's run loop, one shot, the
            // callback captures nothing and is sendable. This owner cancels it.
            self.timer = Some(unsafe {
                NSTimer::scheduledTimerWithTimeInterval_repeats_block(
                    left.as_secs_f64().max(0.001),
                    false,
                    &RcBlock::new(|_| {
                        SCREEN.with(|screen| {
                            if let Some(screen) = screen.borrow_mut().as_mut() {
                                screen.timer.clear();
                                screen.draw();
                            }
                        });
                    }),
                )
            });
        }
    }

    fn clear(&mut self) {
        if let Some(timer) = self.timer.take() {
            timer.invalidate();
        }
        self.at = None;
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.clear();
    }
}
