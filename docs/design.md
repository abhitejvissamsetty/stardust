# Design specification

The browser's interface should feel quiet, fast and exact. The page is the product, so the UI stays small and neutral, and motion is brief.

All values live in [`ui/tokens.css`](../ui/tokens.css). Don't hard-code colors or sizes in components.

## Foundations

| Token group | Values |
|---|---|
| Type | System UI font; 13px for the address, 14px base |
| Spacing | 4, 8 and 12px |
| Radius | 8px for buttons, fully rounded for the address field |
| Motion | 120ms ease-out, turned off for people who ask for reduced motion |
| Themes | Light and dark, following the OS through `prefers-color-scheme` |

## Tab strip

- **Height:** 40px, above the toolbar, on a darker background than the toolbar.
- **Tabs:** 34px tall with rounded top corners, 220px wide, shrinking evenly as more open. The active tab matches the toolbar's surface color and joins it by a 9px curve at each foot.
- **Tab contents:** the page's icon (a globe until it arrives), the title, a speaker button while it plays sound, and a close button. The spinner takes the icon's place while loading. The close button shows on the selected tab and on hover.
- **Unloaded tabs:** a tab with no page loaded, because it was restored or unloaded to save memory, is dimmed.
- **Private badge:** a window that keeps nothing shows a purple "Private" badge at the right of this row.
- **New tabs:** open to the right of the active tab, with the address bar focused.
- **Closing:** the right neighbor shows next, or the left one for the last position. Closing the last tab closes the window.
- **Mouse:** click to switch, drag to reorder, middle-click to close.

## Themes

Light and dark follow the system. A private window is always dark, with a purple accent, set by `data-private` on the page. All values are tokens in `ui/tokens.css`, which also has the colours for warnings and success, and the text colour for use on the accent.

## Toolbar

- **Height:** 48px, with a 1px divider at the bottom.
- **Order:** Back, Forward, Reload or Stop, then the address field filling the remaining width, then pinned extension buttons, the extensions button and the menu button.
- **Buttons:** 32px square with 18px line icons. Hover and press use a translucent fill, so they work on both themes.
- **Disabled buttons:** use the disabled text color, with no hover effect.

## Address field

- **At rest:** a 34px pill showing the page's full URL, with a lock icon for `https://` pages, and an amber "Not secure" label for plain `http://` pages other than local addresses. The lock only reflects the URL scheme, because the certificate details aren't shown yet.
- **Zoom:** a chip with the percentage appears when the page isn't at 100%. Clicking it resets the zoom.
- **Buttons inside:** the saved-login key, when the site has one, and the bookmark star.
- **Focused:** a 2px accent outline, with all text selected so typing replaces it.
- **Enter:** goes to a URL or host name, otherwise searches. The destination shows immediately.
- **Escape:** restores the current URL.
- **Text safety:** page-controlled text is only ever set through `value` or `textContent`, never as HTML.

## Menus

Menus hang under the toolbar, with their right edge under their button, and show one at a time.

- **App menu:** New tab, window and private window; History, Downloads, Bookmarks and Extensions; zoom with a full screen button; Print, Find, Reopen closed tab and Developer tools; Settings; Quit. Rows are 34px with the shortcut on the right.
- **Extensions menu:** each extension that's on, with its icon and a pin button, and "Manage extensions".
- **Extension popups** show under the button, 380 by 520, in a browser of their own. They close on a click elsewhere, a tab switch, a second press of the button, or Escape.
- A click anywhere else in the toolbar, or in the page, puts a menu away.

## Extensions page

`browser://extensions`: a card for each extension with its icon, name, version and description, chips for the strongest things it can do, an on/off switch, and Details, Pin and Remove. Adding one shows a card of what it will be able to do, in plain words, before it's added. A banner offers a restart when changes need one.

## Loading

A 2px accent bar runs along the toolbar's bottom edge, and Reload becomes Stop. The bar disappears when loading ends.

## Find bar

A 44px bar below the toolbar, with its controls aligned right: the text field, a match count, Previous, Next and Close. The field outline turns red when nothing matches. Escape closes it and returns focus to the page.

## Download bar

- **Placement:** a 44px bar below the toolbar, which appears with the first download and goes away when the list is empty.
- **Items:** newest first, up to 280px wide, showing the file name and its status.
- **While downloading:** size, speed, a progress line and a Cancel button.
- **When finished:** Show in folder for completed files, and Remove from list. Clear removes all finished items.
- **Safety:** downloaded files are never opened automatically.

## Errors

Failed loads show a centered, plain page: a heading, the URL and the error code. The address bar keeps the failed URL, and Reload retries it.

## Browser pages

History, Bookmarks and Settings are centered single-column pages, 760px wide at most, using the same tokens. They open from the toolbar buttons to the right of the address field, and from the star inside it for bookmarking.

## Keyboard

| Action | Linux and Windows | macOS |
|---|---|---|
| Focus address | Ctrl+L, Alt+D, F6 | Cmd+L, F6 |
| Back | Alt+Left | Cmd+[, Cmd+Left |
| Forward | Alt+Right | Cmd+], Cmd+Right |
| Reload | Ctrl+R, F5 | Cmd+R, F5 |
| Hard reload | Ctrl+Shift+R, Shift+F5 | Cmd+Shift+R, Shift+F5 |
| Stop loading | Escape | Escape |
| Find in page | Ctrl+F | Cmd+F |
| Next match | F3, Ctrl+G | F3, Cmd+G |
| Previous match | Shift+F3, Ctrl+Shift+G | Shift+F3, Cmd+Shift+G |
| Bookmark this page | Ctrl+D | Cmd+D |
| History | Ctrl+H | Cmd+H |
| Bookmarks | Ctrl+Shift+O | Cmd+Shift+O |
| Settings | Ctrl+, | Cmd+, |
| New tab | Ctrl+T | Cmd+T |
| New window | Ctrl+N | Cmd+N |
| Close tab | Ctrl+W | Cmd+W |
| Next tab | Ctrl+Tab | Ctrl+Tab |
| Previous tab | Ctrl+Shift+Tab | Ctrl+Shift+Tab |
| Tab 1 to 8 | Ctrl+1 to Ctrl+8 | Cmd+1 to Cmd+8 |
| Last tab | Ctrl+9 | Cmd+9 |
| New private window | Ctrl+Shift+N | Cmd+Shift+N |
| Reopen closed tab | Ctrl+Shift+T | Cmd+Shift+T |
| Move tab left or right | Ctrl+Shift+Page Up or Down | Cmd+Shift+Page Up or Down |
| Zoom in, out, reset | Ctrl + `+`, `-`, `0` | Cmd + `+`, `-`, `0` |
| Print | Ctrl+P | Cmd+P |
| Developer tools | F12, Ctrl+Shift+I | Cmd+Option+I |
| Full screen | F11 | Ctrl+Cmd+F |
| Minimize | | Cmd+M |
| Quit, closing all windows | Ctrl+Q | Cmd+Q |

## Accessibility

Every control has an accessible name, can be reached by keyboard, and shows a visible focus ring.
