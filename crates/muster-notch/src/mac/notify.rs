//! Notifications of the Mac's own (`PRODUCT.md` §6): from Muster's helper
//! running as Muster's app, which macOS asks the person to allow once; a
//! click on one asks for its session to be shown in the Muster window
//! looked at last. A program that is not an app has no notifications: the
//! link is told so, and the UIs' terminals tell the person instead.

use block2::RcBlock;
use muster_core::SessionId;
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_foundation::{NSBundle, NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNNotificationSound, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use tokio::sync::mpsc;

use crate::link::FromScreen;

define_class!(
    // SAFETY: NSObject asks nothing more of a subclass than that its
    // methods keep the protocol's signatures, as these do; its one ivar is
    // a channel's sender, which is `Send` and `Sync`, as a delegate called
    // from any thread needs, and it has no `Drop` of its own.
    #[unsafe(super(NSObject))]
    #[name = "MusterNotifyDelegate"]
    #[ivars = mpsc::Sender<FromScreen>]
    struct NotifyDelegate;

    unsafe impl NSObjectProtocol for NotifyDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for NotifyDelegate {
        // SAFETY: the protocol's `userNotificationCenter:
        // didReceiveNotificationResponse:withCompletionHandler:`, its
        // handler called once, as it asks.
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            done: &block2::DynBlock<dyn Fn()>,
        ) {
            let identifier = response.notification().request().identifier().to_string();
            if let Ok(session) = identifier.parse::<SessionId>() {
                let _ = self.ivars().try_send(FromScreen::Picked(session));
            }
            done.call(());
        }

        // SAFETY: the protocol's `userNotificationCenter:
        // willPresentNotification:withCompletionHandler:`, its handler
        // called once: shown as a banner, with its sound.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            done: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            done.call((
                UNNotificationPresentationOptions::Banner | UNNotificationPresentationOptions::Sound,
            ));
        }
    }
);

impl NotifyDelegate {
    fn new(said: mpsc::Sender<FromScreen>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(said);
        // SAFETY: NSObject's designated `init`, which takes nothing more.
        unsafe { msg_send![super(this), init] }
    }
}

/// Muster's notifications, while the program is Muster's app.
pub(super) struct Notifier {
    center: Retained<UNUserNotificationCenter>,
    /// Held: the centre keeps its delegate only weakly.
    _delegate: Retained<NotifyDelegate>,
}

impl Notifier {
    /// Asks the person, the first time, to allow Muster's notifications;
    /// whether they did comes to the link through `said`. `None`, and the
    /// link told it cannot notify, where the program is not Muster's app.
    pub(super) fn start(said: &mpsc::Sender<FromScreen>) -> Option<Self> {
        if NSBundle::mainBundle().bundleIdentifier().is_none() {
            let _ = said.try_send(FromScreen::CanNotify(false));
            return None;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let delegate = NotifyDelegate::new(said.clone());
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        let answer = said.clone();
        let answered = RcBlock::new(move |allowed: Bool, _error: *mut NSError| {
            let _ = answer.try_send(FromScreen::CanNotify(allowed.as_bool()));
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &answered,
        );
        Some(Self {
            center,
            _delegate: delegate,
        })
    }

    /// Tells the person `text` of `session`: one notification a session,
    /// the newest in place of the one before.
    pub(super) fn post(&self, session: SessionId, text: &str) {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str("Muster"));
        content.setBody(&NSString::from_str(text));
        content.setSound(Some(&UNNotificationSound::defaultSound()));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&session.to_string()),
            &content,
            None,
        );
        self.center
            .addNotificationRequest_withCompletionHandler(&request, None);
    }
}
