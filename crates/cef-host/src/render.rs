//! Runs in renderer processes. Exposes `window.browserBridge.send()`, which forwards
//! a JSON string to the browser process. Every page gets it, because renderers can't
//! be trusted to decide who may use it. The browser process only listens to messages
//! from the toolbar's own client, so it does nothing in a web page.

use cef::*;

pub const UI_COMMAND_MESSAGE: &str = "ui-command";

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
            if frame.is_main() == 0 || !CefString::from(&frame.url()).to_string().starts_with("data:") {
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
