//! Runs in renderer processes.
//!
//! - Browser pages (`data:` URLs) get `window.browserBridge.send()`, which forwards a
//!   JSON string to the browser process. The browser decides whom to believe, so a
//!   page with the bridge that isn't one of its own gets nowhere.
//! - Web pages get a watcher that reports logins being sent, so the browser can offer
//!   to save them, and can be asked to fill a saved login.

use bridge::{CLIP_SCRIPT, LOGIN_WATCHER, fill_script, is_web_url, login_origin};
use cef::*;

pub const UI_COMMAND_MESSAGE: &str = "ui-command";
pub const LOGIN_SUBMITTED_MESSAGE: &str = "login-submitted";
pub const FILL_LOGIN_MESSAGE: &str = "fill-login";
/// The browser asks a web page what a clip should keep from it, and the page answers.
pub const CLIP_REQUEST_MESSAGE: &str = "clip-request";
pub const CLIP_RESULT_MESSAGE: &str = "clip-result";

wrap_render_process_handler! {
    pub struct BridgeInstaller;

    impl RenderProcessHandler {
        fn on_context_created(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            context: Option<&mut V8Context>,
        ) {
            let (Some(frame), Some(context)) = (frame, context) else {
                return;
            };
            let url = CefString::from(&frame.url()).to_string();
            if frame.is_main() != 0 && login_origin(&url).is_some() {
                install_login_watcher(context);
                return;
            }
            if frame.is_main() == 0 || !url.starts_with("data:") {
                return;
            }
            let Some(global) = context.global() else {
                return;
            };

            let mut handler = BridgeSend::new();
            let name = CefString::from("send");
            let Some(mut send) = v8_value_create_function(Some(&name), Some(&mut handler)) else {
                return;
            };
            let Some(mut bridge) = v8_value_create_object(None, None) else {
                return;
            };
            bridge.set_value_bykey(Some(&name), Some(&mut send), V8Propertyattribute::default());
            global.set_value_bykey(
                Some(&CefString::from("browserBridge")),
                Some(&mut bridge),
                V8Propertyattribute::default(),
            );
        }

        // The browser asks to fill a login. The frame must still be the site the login
        // belongs to, since it could have moved on since the browser decided.
        fn on_process_message_received(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> i32 {
            let (Some(frame), Some(message)) = (frame, message) else {
                return 0;
            };
            let name = CefString::from(&message.name()).to_string();
            if name == CLIP_REQUEST_MESSAGE {
                answer_clip_request(frame);
                return 1;
            }
            if name != FILL_LOGIN_MESSAGE {
                return 0;
            }
            let Some(list) = message.argument_list() else {
                return 1;
            };
            let argument = |index| CefString::from(&list.string(index)).to_string();
            let url = CefString::from(&frame.url()).to_string();
            if frame.is_main() != 0 && login_origin(&url).as_deref() == Some(argument(0).as_str()) {
                let script = fill_script(&argument(1), &argument(2));
                frame.execute_java_script(Some(&CefString::from(script.as_str())), None, 0);
            }
            1
        }
    }
}

/// Reads the selection and description of a web page's main frame, and sends them back.
/// An empty answer means nothing could be read.
fn answer_clip_request(frame: &mut Frame) {
    let url = CefString::from(&frame.url()).to_string();
    let mut json = String::new();
    if frame.is_main() != 0
        && is_web_url(&url)
        && let Some(context) = frame.v8_context()
        && context.enter() != 0
    {
        let mut value = None;
        let mut exception = None;
        let evaluated = context.eval(
            Some(&CefString::from(CLIP_SCRIPT)),
            None,
            0,
            Some(&mut value),
            Some(&mut exception),
        );
        if let Some(value) = value.filter(|_| evaluated != 0) {
            json = CefString::from(&value.string_value()).to_string();
        }
        context.exit();
    }
    if let Some(mut message) = process_message_create(Some(&CefString::from(CLIP_RESULT_MESSAGE)))
        && let Some(list) = message.argument_list()
    {
        list.set_string(0, Some(&CefString::from(json.as_str())));
        frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
    }
}

/// Starts the login watcher. It's handed its reporting function directly, so the
/// page's scripts can't find or call it.
fn install_login_watcher(context: &mut V8Context) {
    let mut watcher = None;
    let mut exception = None;
    let evaluated = context.eval(
        Some(&CefString::from(LOGIN_WATCHER)),
        None,
        0,
        Some(&mut watcher),
        Some(&mut exception),
    );
    let Some(watcher) = watcher.filter(|_| evaluated != 0) else {
        return;
    };
    let mut handler = LoginReport::new();
    let name = CefString::from("report");
    let report = v8_value_create_function(Some(&name), Some(&mut handler));
    watcher.execute_function(None, Some(&[report]));
}

wrap_v8_handler! {
    struct LoginReport {}

    impl V8Handler {
        fn execute(
            &self,
            _name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            _retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> i32 {
            let json = arguments
                .and_then(|arguments| arguments.first())
                .and_then(Option::as_ref)
                .map(|value| CefString::from(&value.string_value()));
            let frame = v8_context_get_current_context().and_then(|context| context.frame());
            let message = process_message_create(Some(&CefString::from(LOGIN_SUBMITTED_MESSAGE)));
            if let (Some(json), Some(frame), Some(mut message)) = (json, frame, message)
                && let Some(list) = message.argument_list()
            {
                list.set_string(0, Some(&json));
                frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
            }
            1
        }
    }
}

wrap_v8_handler! {
    struct BridgeSend {}

    impl V8Handler {
        fn execute(
            &self,
            _name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            _retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> i32 {
            let json = arguments
                .and_then(|arguments| arguments.first())
                .and_then(Option::as_ref)
                .map(|value| CefString::from(&value.string_value()));
            let frame = v8_context_get_current_context().and_then(|context| context.frame());
            let message = process_message_create(Some(&CefString::from(UI_COMMAND_MESSAGE)));
            if let (Some(json), Some(frame), Some(mut message)) = (json, frame, message)
                && let Some(list) = message.argument_list()
            {
                list.set_string(0, Some(&json));
                frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
            }
            1
        }
    }
}
