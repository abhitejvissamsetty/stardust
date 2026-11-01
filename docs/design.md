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
- **Tabs:** 220px wide, shrinking evenly as more open. The active tab matches the toolbar's surface color.
- **Tab contents:** a spinner while loading, the page title, and a close button.
- **New tabs:** open to the right of the active tab, with the address bar focused.
- **Closing:** the right neighbor shows next, or the left one for the last position. Closing the last tab closes the window.
- **Mouse:** click to switch, drag to reorder, middle-click to close.

## Toolbar

- **Height:** 48px, with a 1px divider at the bottom.
- **Order:** Back, Forward, Reload or Stop, then the address field filling the remaining width.
- **Buttons:** 32px square with 18px line icons. Hover and press use a translucent fill, so they work on both themes.
- **Disabled buttons:** use the disabled text color, with no hover effect.

## Address field

- **At rest:** a pill showing the page's full URL, with a lock icon for `https://` pages. The lock only reflects the URL scheme, because the certificate details aren't shown yet.
- **Focused:** a 2px accent outline, with all text selected so typing replaces it.
- **Enter:** goes to a URL or host name, otherwise searches. The destination shows immediately.
- **Escape:** restores the current URL.
- **Text safety:** page-controlled text is only ever set through `value` or `textContent`, never as HTML.

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
| Quit, closing all windows | Ctrl+Q | Cmd+Q |

## Accessibility

Every control has an accessible name, can be reached by keyboard, and shows a visible focus ring.
