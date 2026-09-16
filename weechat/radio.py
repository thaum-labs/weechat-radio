#!/usr/bin/env python3
# -*- coding: utf-8 -*-
#
# Copyright (C) 2026 Thaum Labs
#
# This program is free software; you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation; either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
#
"""WeeChat Radio helper: status bar, /radio alias, delivery ticks, airtime."""

import json
import urllib.request

try:
    import weechat
except ImportError:
    raise Exception("this script must be loaded inside WeeChat")

SCRIPT_NAME = "radio"
SCRIPT_AUTHOR = "Thaum Labs"
SCRIPT_VERSION = "0.1.0"
SCRIPT_LICENSE = "GPL3"
SCRIPT_DESC = "WeeChat Radio status bar, /radio command, delivery ticks"

STATUS_URL = "http://127.0.0.1:8074/status"

MODE_COLOR = {
    "internet": "cyan",
    "internet-radio": "green",
    "radio": "yellow",
    "radio-plus": "magenta",
}

TICKS = {
    "queued": "·",
    "sent": "✓",
    "relayed": "✓✓",
    "delivered": "✓✓",
    "all": "✓✓✓",
}


def _status():
    try:
        with urllib.request.urlopen(STATUS_URL, timeout=0.4) as r:
            return json.loads(r.read().decode("utf-8"))
    except Exception:
        return {}


def bar_item_cb(*_args):
    s = _status()
    if not s:
        return weechat.color("red") + "wcr down"
    mode = s.get("mode", "?")
    col = MODE_COLOR.get(mode, "default")
    banner = s.get("hub_banner") or ""
    hub = "hub↑" if s.get("hub_ok") else "hub↓"
    audio = s.get("audio_label", "?")
    q = s.get("queue_out", 0)
    snr = s.get("snr", 0)
    freq = s.get("frequency") or s.get("preset", "")
    ptt = "TX" if s.get("ptt_on") else s.get("channel", "idle")
    upd = " UPD" if s.get("update_available") else ""
    text = " %s %s %s SNR %.0f %s q%s %s%s " % (
        mode,
        ptt,
        freq,
        snr,
        audio,
        q,
        hub,
        upd,
    )
    if banner:
        return weechat.color("black,yellow") + " " + banner + " " + weechat.color(col) + text
    return weechat.color(col) + text


def radio_cmd(data, buffer, args):
    server = weechat.buffer_get_string(buffer, "localvar_server")
    weechat.command(buffer, "/quote %s RADIO %s" % (server or "radio", args))
    return weechat.WEECHAT_RC_OK


def input_cb(data, modifier, modifier_data, string):
    n = len(string)
    weechat.bar_item_update("radio_air")
    weechat.config_set_plugin("draft_len", str(n))
    return string


def air_cb(*_args):
    n = int(weechat.config_get_plugin("draft_len") or "0")
    return "%d B" % n


def tagmsg_cb(data, signal, signal_data):
    # signal_data is the raw IRC line; look for radio/delivery
    line = signal_data or ""
    if "radio/delivery=" not in line:
        return weechat.WEECHAT_RC_OK
    state = "sent"
    try:
        state = line.split("radio/delivery=")[1].split(";")[0].split(" ")[0]
    except Exception:
        pass
    tick = TICKS.get(state, "")
    weechat.prnt("", "%s delivery %s" % (weechat.prefix("network"), tick + " " + state))
    return weechat.WEECHAT_RC_OK


if weechat.register(SCRIPT_NAME, SCRIPT_AUTHOR, SCRIPT_VERSION, SCRIPT_LICENSE, SCRIPT_DESC, "", ""):
    weechat.bar_item_new("radio", "bar_item_cb", "")
    weechat.bar_item_new("radio_air", "air_cb", "")
    weechat.hook_command(
        "radio",
        "WeeChat Radio node command (mode, preset, group, …)",
        "<subcommand> [args]",
        "See /radio help on the radio server for the list.",
        "",
        "radio_cmd",
        "",
    )
    weechat.hook_modifier("input_text_content", "input_cb", "")
    weechat.hook_signal("*,irc_in_TAGMSG", "tagmsg_cb", "")
    weechat.hook_timer(4000, 0, 0, "bar_item_cb", "")
    weechat.command("", "/bar add radio_bar window bottom 1 0 radio,radio_air")
    weechat.config_set_plugin("draft_len", "0")
