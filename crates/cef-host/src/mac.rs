//! The `NSApplication` subclass that CEF requires on macOS.

use std::cell::Cell;

use cef::application_mac::{CefAppProtocol, CrAppControlProtocol, CrAppProtocol};
use objc2::{
    ClassType, DefinedClass, MainThreadMarker, define_class, extern_methods, msg_send,
    rc::Retained,
    runtime::{AnyObject, Bool, NSObjectProtocol},
};
use objc2_app_kit::{NSApp, NSApplication, NSEvent};

#[derive(Default)]
pub struct BrowserApplicationIvars {
    handling_send_event: Cell<Bool>,
}

define_class!(
    #[unsafe(super(NSApplication))]
    #[ivars = BrowserApplicationIvars]
    pub struct BrowserApplication;

    impl BrowserApplication {
        #[unsafe(method(sendEvent:))]
        unsafe fn send_event(&self, event: &NSEvent) {
            let was_sending_event = self.is_handling_send_event();
            if !was_sending_event {
                self.set_handling_send_event(true);
            }

            let _: () = msg_send![super(self), sendEvent:event];

            if !was_sending_event {
                self.set_handling_send_event(false);
            }
        }

        /// The default implementation calls `exit()`, but Chromium must leave its
        /// message loop to shut down in order. Closing every browser does that.
        #[unsafe(method(terminate:))]
        unsafe fn terminate(&self, _sender: &AnyObject) {
            crate::client::close_all_browsers();
        }
    }

    unsafe impl CrAppControlProtocol for BrowserApplication {
        #[unsafe(method(setHandlingSendEvent:))]
        unsafe fn _set_handling_send_event(&self, handling_send_event: Bool) {
            self.ivars().handling_send_event.set(handling_send_event);
        }
    }

    unsafe impl CrAppProtocol for BrowserApplication {
        #[unsafe(method(isHandlingSendEvent))]
        unsafe fn _is_handling_send_event(&self) -> Bool {
            self.ivars().handling_send_event.get()
        }
    }

    unsafe impl CefAppProtocol for BrowserApplication {}
);

impl BrowserApplication {
    extern_methods! {
        #[unsafe(method(sharedApplication))]
        fn shared_application() -> Retained<Self>;

        #[unsafe(method(setHandlingSendEvent:))]
        fn set_handling_send_event(&self, handling_send_event: bool);

        #[unsafe(method(isHandlingSendEvent))]
        fn is_handling_send_event(&self) -> bool;
    }
}

/// Must run before anything touches `NSApp`, or it won't be a `BrowserApplication`.
pub fn setup_application() {
    let _ = BrowserApplication::shared_application();
    let mtm = MainThreadMarker::new().expect("Not running on the main thread");
    assert!(NSApp(mtm).isKindOfClass(BrowserApplication::class()));
}

/// Puts a PNG picture on the system clipboard, so it pastes as a picture. False if the
/// clipboard wouldn't take it. Call on the main thread.
pub fn copy_png(png: &[u8]) -> bool {
    use std::ffi::c_void;

    use objc2::{class, msg_send};

    // SAFETY: plain Cocoa calls on the main thread, with a byte buffer that outlives them.
    // `dataWithBytes:length:` copies the bytes before it returns.
    unsafe {
        let pasteboard: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        if pasteboard.is_null() {
            return false;
        }
        let data: *mut AnyObject = msg_send![
            class!(NSData),
            dataWithBytes: png.as_ptr().cast::<c_void>(),
            length: png.len()
        ];
        let kind: *mut AnyObject = msg_send![
            class!(NSString),
            stringWithUTF8String: c"public.png".as_ptr()
        ];
        if data.is_null() || kind.is_null() {
            return false;
        }
        let _: isize = msg_send![pasteboard, clearContents];
        let stored: Bool = msg_send![pasteboard, setData: data, forType: kind];
        stored.as_bool()
    }
}

/// Puts plain text on the system clipboard. Call on the main thread.
pub fn copy_text(text: &str) -> bool {
    use objc2::{class, msg_send};
    use std::ffi::CString;

    let Ok(text) = CString::new(text) else {
        return false;
    };
    // SAFETY: plain Cocoa calls on the main thread with NUL-terminated strings that
    // outlive them.
    unsafe {
        let pasteboard: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        let value: *mut AnyObject =
            msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()];
        let kind: *mut AnyObject =
            msg_send![class!(NSString), stringWithUTF8String: c"public.utf8-plain-text".as_ptr()];
        if pasteboard.is_null() || value.is_null() || kind.is_null() {
            return false;
        }
        let _: isize = msg_send![pasteboard, clearContents];
        let stored: Bool = msg_send![pasteboard, setString: value, forType: kind];
        stored.as_bool()
    }
}
