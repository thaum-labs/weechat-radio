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

# 256-colour matches for the site / TUI shell.
MODE_COLOR = {
    "internet": "51",
    "internet-radio": "46",
    "radio": "220",
    "radio-plus": "207",
}

TICKS = {
    "queued": "·",
    "sent": "✓",
    "relayed": "✓✓",
    "delivered": "✓✓",
    "all": "✓✓✓",
}

ACCENT = "111"
ORANGE = "209"
DIM = "243"

CHROME = (
    "/set weechat.bar.status.color_bg 232",
    "/set weechat.bar.status.color_fg 111",
    "/set weechat.bar.input.color_bg 232",
    "/set weechat.bar.input.color_fg 111",
    "/set weechat.bar.input.color_delim 209",
    "/set weechat.bar.title.color_bg 232",
    "/set weechat.bar.title.color_fg 111",
    "/set weechat.bar.buflist.color_fg 111",
    "/set weechat.bar.buflist.color_bg 232",
    "/set weechat.bar.nicklist.color_fg 111",
    "/set weechat.bar.nicklist.color_bg 232",
    "/set weechat.bar.nicklist.separator on",
    "/set weechat.bar.radio_bar.color_fg 111",
    "/set weechat.bar.radio_bar.color_bg 232",
    "/set weechat.look.color_inactive_window 243",
    "/set weechat.look.color_nicklist_away 243",
    "/set weechat.color.chat 189",
    "/set weechat.color.chat_time 243",
    "/set weechat.color.chat_time_delimiters 243",
    "/set weechat.color.chat_nick 111",
    "/set weechat.color.chat_nick_self 209",
    "/set weechat.color.chat_prefix_network 209",
    "/set weechat.color.chat_prefix_join 46",
    "/set weechat.color.chat_highlight 209,232",
    "/set weechat.color.separator 243",
    "/set weechat.color.status_name 111",
    "/set weechat.color.status_name_insecure 209",
    "/set weechat.color.status_time 243",
    "/set weechat.color.status_data_msg 111",
)


def _theme():
    return weechat.config_get_plugin("theme") or "tron"


def _col(name):
    if _theme() == "plain":
        return ""
    try:
        return weechat.color(str(name)) or ""
    except Exception:
        return ""


def _status():
    try:
        with urllib.request.urlopen(STATUS_URL, timeout=0.4) as r:
            return json.loads(r.read().decode("utf-8"))
    except Exception:
        return {}


def bar_item_cb(*_args):
    try:
        s = _status()
        if not s:
            return _col("red") + "WCR DOWN"
        mode = s.get("mode", "?")
        mode_col = MODE_COLOR.get(mode, "default")
        banner = s.get("hub_banner") or ""
        hub_ok = bool(s.get("hub_ok"))
        hub = (
            _col("46") + "HUB UP"
            if hub_ok
            else _col(ORANGE) + "HUB DOWN"
        )
        audio = (s.get("audio_label") or "?").upper()
        q = s.get("queue_out", 0)
        snr = s.get("snr") or 0
        freq = s.get("frequency") or s.get("preset", "")
        if s.get("ptt_on"):
            ptt = _col(ORANGE) + "TX"
        else:
            ptt = _col(DIM) + (s.get("channel") or "idle").upper()
        upd = " │ UPD" if s.get("update_available") else ""
        sep = " │ "
        base = _col(ACCENT)
        token = _col(mode_col) + mode.upper() + base
        text = (
            " "
            + token
            + sep
            + ptt
            + sep
            + str(freq).upper()
            + sep
            + "SNR %.0f" % float(snr)
            + sep
            + audio
            + sep
            + "Q%s" % q
            + sep
            + hub
            + upd
            + " "
        )
        if banner:
            return _col("black,yellow") + " " + banner + " " + base + text
        return base + text
    except Exception:
        return " WCR "


def timer_cb(_data, _remaining):
    weechat.bar_item_update("radio")
    return weechat.WEECHAT_RC_OK


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
    line = signal_data or ""
    if "radio/delivery=" not in line:
        return weechat.WEECHAT_RC_OK
    state = "sent"
    try:
        state = line.split("radio/delivery=")[1].split(";")[0].split(" ")[0]
    except Exception:
        pass
    tick = TICKS.get(state, "")
    weechat.prnt(
        "",
        "%s%s delivery %s"
        % (weechat.prefix("network"), _col(ACCENT), tick + " " + state),
    )
    return weechat.WEECHAT_RC_OK


if weechat.register(SCRIPT_NAME, SCRIPT_AUTHOR, SCRIPT_VERSION, SCRIPT_LICENSE, SCRIPT_DESC, "", ""):
    if not weechat.config_is_set_plugin("theme"):
        weechat.config_set_plugin("theme", "tron")
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
    weechat.hook_timer(4000, 0, 0, "timer_cb", "")
    if not weechat.bar_search("radio_bar"):
        weechat.command("", "/bar add radio_bar window bottom 1 0 radio,radio_air")
    if _theme() == "tron":
        for cmd in CHROME:
            weechat.command("", cmd)
    weechat.config_set_plugin("draft_len", "0")
