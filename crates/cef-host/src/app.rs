use std::cell::RefCell;

use bridge::{Session, TabId};
use cef::*;

use crate::{
    client::{
        SharedShell, ToolbarClient, adopt_menu_view, clear_window, load_background_tabs,
        prepare_tabs, register_shell, schedule_discard_check, select_tab, set_window,
        startup_windows, toolbar_height, try_close_all, window_activated,
    },
    render::BridgeInstaller,
    ui::toolbar_url,
};

wrap_app! {
    pub struct BrowserApp;

    impl App {
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(ProcessHandler::new())
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(BridgeInstaller::new())
        }
    }
}

wrap_browser_process_handler! {
    struct ProcessHandler;

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            debug_assert_ne!(currently_on(ThreadId::UI), 0);

            let command_line = command_line_get_global().expect("Failed to get command line");
            let url = CefString::from(&command_line.switch_value(Some(&CefString::from("url"))))
                .to_string();
            let mut arguments = CefStringList::new();
            command_line.arguments(Some(&mut arguments));
            let requested = requested_urls(
                Some(url).into_iter().chain(arguments.into_iter().map(|arg| arg.to_string())),
            );

            let windows = startup_windows();
            let window_count = windows.len();
            for (index, session) in windows.into_iter().enumerate() {
                // Requested addresses open as new tabs in the last window.
                let is_last = index + 1 == window_count;
                open_window(Some(session), if is_last { &requested } else { &[] });
            }
            if window_count == 0 {
                open_window(None, &requested);
            }
            schedule_discard_check();
        }
    }
}

/// Addresses given on the command line, like `stardust example.com https://a.org/`.
/// Only web addresses are opened, so a command line can't reach internal pages.
fn requested_urls(inputs: impl Iterator<Item = String>) -> Vec<String> {
    inputs
        .filter_map(|input| bridge::resolve_input(&input, "search:"))
        .filter(|url| bridge::is_web_url(url) || url == "about:blank")
        .collect()
}

/// Opens a window. Restored tabs load when selected, starting with the first one shown.
/// Requested addresses open as tabs that load straight away.
pub fn open_window(session: Option<Session>, requested_urls: &[String]) {
    let shell = SharedShell::default();
    register_shell(&shell);

    let mut toolbar_client = ToolbarClient::new(shell.clone());
    let toolbar = browser_view_create(
        Some(&mut toolbar_client),
        Some(&CefString::from(toolbar_url().as_str())),
        Some(&BrowserSettings::default()),
        None,
        None,
        Some(&mut ToolbarViewHost::new(shell.clone())),
    );
    let first_tab = prepare_tabs(&shell, session, requested_urls);

    window_create_top_level(Some(&mut WindowHost::new(
        shell,
        RefCell::new(toolbar),
        first_tab,
    )));
}

wrap_browser_view_delegate! {
    struct ToolbarViewHost {
        shell: SharedShell,
    }

    impl ViewDelegate {
        // CEF ignores a preferred size with a width of 0.
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 1280, height: toolbar_height(&self.shell) }
        }
    }

    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }

        fn delegate_for_popup_browser_view(
            &self,
            _browser_view: Option<&mut BrowserView>,
            _settings: Option<&BrowserSettings>,
            _client: Option<&mut Client>,
            _is_devtools: i32,
        ) -> Option<BrowserViewDelegate> {
            Some(MenuViewHost::new())
        }

        // The only window the toolbar may open is its menu.
        fn on_popup_browser_view_created(
            &self,
            _browser_view: Option<&mut BrowserView>,
            popup_browser_view: Option<&mut BrowserView>,
            _is_devtools: i32,
        ) -> i32 {
            popup_browser_view
                .is_some_and(|view| adopt_menu_view(&self.shell, view))
                .into()
        }
    }
}

wrap_browser_view_delegate! {
    struct MenuViewHost {}

    impl ViewDelegate {}

    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

wrap_window_delegate! {
    struct WindowHost {
        shell: SharedShell,
        toolbar: RefCell<Option<BrowserView>>,
        first_tab: TabId,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 1280, height: 800 }
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let toolbar = self.toolbar.borrow();
            let (Some(window), Some(toolbar)) = (window, toolbar.as_ref()) else {
                return;
            };

            let settings = BoxLayoutSettings {
                cross_axis_alignment: AxisAlignment::STRETCH,
                ..Default::default()
            };
            let layout = window.set_to_box_layout(Some(&settings));
            window.add_child_view(Some(&mut View::from(toolbar)));
            set_window(&self.shell, window.clone(), layout);
            select_tab(&self.shell, self.first_tab);
            load_background_tabs(&self.shell);
            window.show();
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            *self.toolbar.borrow_mut() = None;
            clear_window(&self.shell);
        }

        fn can_close(&self, _window: Option<&mut Window>) -> i32 {
            try_close_all(&self.shell).into()
        }

        fn on_window_activation_changed(&self, _window: Option<&mut Window>, active: i32) {
            if active != 0 {
                window_activated(&self.shell);
            }
        }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}
