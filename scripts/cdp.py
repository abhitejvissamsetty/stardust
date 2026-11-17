#!/usr/bin/env python3
"""Drives a running Stardust through Chromium's debugging protocol, for testing.

Start the browser with `--remote-debugging-port=9333`, then:

    python3 scripts/cdp.py list
    python3 scripts/cdp.py eval Toolbar 'document.getElementById("address").value'
    python3 scripts/cdp.py type Toolbar 'example'
    python3 scripts/cdp.py shot Toolbar toolbar.png

`eval` runs JavaScript in the page with that title and prints the result. `type`
sends real key presses to the page's focused element, which pages treat as a
person typing. `shot` saves a PNG of the page, which is how Stardust's own
interface (toolbar, tabs, menus, Settings) can be looked at without the screen. Pass `--port` before the command to use another port.

Uses only the standard library, so it runs anywhere Python 3 does.
"""

import base64
import json
import os
import socket
import struct
import sys
import urllib.request


class Session:
    """A debugging connection to one page."""

    def __init__(self, port, title):
        targets = json.load(urllib.request.urlopen(f"http://127.0.0.1:{port}/json/list"))
        matches = [target for target in targets if target["title"] == title]
        if not matches:
            titles = ", ".join(repr(target["title"]) for target in targets)
            sys.exit(f"No page titled {title!r}. Pages: {titles}")
        path = matches[0]["webSocketDebuggerUrl"].split(str(port), 1)[1]
        self.sock = socket.create_connection(("127.0.0.1", port))
        key = base64.b64encode(os.urandom(16)).decode()
        self.sock.send(
            f"GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\n"
            f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\n"
            "Sec-WebSocket-Version: 13\r\n\r\n".encode()
        )
        response = b""
        while b"\r\n\r\n" not in response:
            response += self.sock.recv(1)
        self.next_id = 0

    def _send(self, message):
        data = json.dumps(message).encode()
        mask = os.urandom(4)
        header = bytes([0x81])
        if len(data) < 126:
            header += bytes([0x80 | len(data)])
        else:
            header += bytes([0x80 | 126]) + struct.pack(">H", len(data))
        self.sock.send(header + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

    def _exact(self, count):
        data = b""
        while len(data) < count:
            data += self.sock.recv(count - len(data))
        return data

    def _receive(self):
        _, second = self._exact(2)
        length = second & 0x7F
        if length == 126:
            length = struct.unpack(">H", self._exact(2))[0]
        elif length == 127:
            length = struct.unpack(">Q", self._exact(8))[0]
        return json.loads(self._exact(length))

    def call(self, method, params=None):
        """Sends a protocol command and returns its result."""
        self.next_id += 1
        self._send({"id": self.next_id, "method": method, "params": params or {}})
        while True:
            reply = self._receive()
            if reply.get("id") == self.next_id:
                return reply.get("result", reply)

    def evaluate(self, expression, user_gesture=False):
        """Runs JavaScript and returns its value."""
        result = self.call(
            "Runtime.evaluate",
            {"expression": expression, "returnByValue": True, "userGesture": user_gesture},
        )
        return result.get("result", {}).get("value", result)

    def screenshot(self, path, width=None, height=None):
        """Saves a PNG of the page, for looking at Stardust's own interface."""
        params = {"format": "png"}
        if width and height:
            params["clip"] = {"x": 0, "y": 0, "width": width, "height": height, "scale": 1}
        data = self.call("Page.captureScreenshot", params)["data"]
        with open(path, "wb") as file:
            file.write(base64.b64decode(data))

    def type(self, text):
        """Sends key presses for each character."""
        for char in text:
            self.call("Input.dispatchKeyEvent", {"type": "keyDown", "text": char, "key": char})
            self.call("Input.dispatchKeyEvent", {"type": "keyUp", "key": char})


def main(args):
    port = 9333
    if args[:1] == ["--port"]:
        port, args = int(args[1]), args[2:]
    if args[:1] == ["list"]:
        for target in json.load(urllib.request.urlopen(f"http://127.0.0.1:{port}/json/list")):
            print(f"{target['type']:<8} {target['title']!r:<30} {target['url'][:60]}")
    elif len(args) == 3 and args[0] == "eval":
        print(json.dumps(Session(port, args[1]).evaluate(args[2])))
    elif len(args) == 3 and args[0] == "shot":
        Session(port, args[1]).screenshot(args[2])
    elif len(args) == 3 and args[0] == "type":
        Session(port, args[1]).type(args[2])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
