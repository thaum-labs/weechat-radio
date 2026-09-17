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

# Applied via the config API so WeeChat does not dump "/set" into the chat.
CHROME = (
    ("weechat.startup.display_logo", "off"),
    ("weechat.startup.display_version", "off"),
    ("logger.level.core", "0"),
    ("irc.look.server_buffer", "independent"),
    ("irc.look.display_host_join", "off"),
    ("irc.look.display_host_quit", "off"),
    ("irc.server.radio.autojoin", "#bulletin"),
    ("weechat.bar.status.color_bg", "232"),
    ("weechat.bar.status.color_fg", "111"),
    ("weechat.bar.input.color_bg", "232"),
    ("weechat.bar.input.color_fg", "111"),
    ("weechat.bar.input.color_delim", "209"),
    ("weechat.bar.title.color_bg", "232"),
    ("weechat.bar.title.color_fg", "111"),
    ("weechat.bar.buflist.color_fg", "111"),
    ("weechat.bar.buflist.color_bg", "232"),
    ("weechat.bar.nicklist.color_fg", "111"),
    ("weechat.bar.nicklist.color_bg", "232"),
    ("weechat.bar.nicklist.separator", "on"),
    ("weechat.bar.radio_bar.color_fg", "111"),
    ("weechat.bar.radio_bar.color_bg", "232"),
    ("weechat.color.chat_inactive_window", "243"),
    ("weechat.color.nicklist_away", "243"),
    ("weechat.color.chat", "189"),
    ("weechat.color.chat_time", "243"),
    ("weechat.color.chat_time_delimiters", "243"),
    ("weechat.color.chat_nick", "111"),
    ("weechat.color.chat_nick_self", "209"),
    ("weechat.color.chat_prefix_network", "209"),
    ("weechat.color.chat_prefix_join", "46"),
    ("weechat.color.chat_highlight", "209"),
    ("weechat.color.chat_highlight_bg", "232"),
    ("weechat.color.separator", "243"),
    ("weechat.color.status_name", "111"),
    ("weechat.color.status_name_insecure", "209"),
    ("weechat.color.status_time", "243"),
    ("weechat.color.status_data_msg", "111"),
)

BANNER = (
    " __      _____ ___ ___ _  _   _ _____    ___    _   ___ ___ ___",
    " \\ \\    / / __| __/ __| || | /_\\_   _|  | _ \\  /_\\ |   \\_ _/ _ \\",
    "  \\ \\/\\/ /| _|| _| (__| __ |/ _ \\| |    |   / / _ \\| |) | | (_) |",
    "   \\_/\\_/ |___|___\\___|_||_/_/ \\_\\_|    |_|_\\/_/ \\_\\___/___\\___/",
)


_banner_shown = False


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


def _set(name, value):
    ptr = weechat.config_get(name)
    if ptr:
        weechat.config_option_set(ptr, value, 1)


def _mute(cmd):
    weechat.command("", "/mute -all " + cmd)


def apply_chrome():
    if _theme() != "tron":
        return
    for name, value in CHROME:
        _set(name, value)
    _mute("/filter addreplace wcr_cap * irc_cap *")
    _mute("/filter addreplace wcr_motd * irc_372,irc_375,irc_376,irc_422 *")
    _mute("/filter addreplace wcr_welcome * irc_001,irc_002,irc_003,irc_004,irc_005 *")


def _clear_buffer(plugin, name):
    buf = weechat.buffer_search(plugin, name)
    if buf:
        weechat.command(buf, "/mute /buffer clear")


def _print_banner(buf):
    global _banner_shown
    if _banner_shown or not buf:
        return
    name = weechat.buffer_get_string(buf, "full_name") or ""
    if "bulletin" not in name:
        return
    _banner_shown = True
    weechat.prnt(buf, "")
    for line in BANNER:
        weechat.prnt(buf, "%s%s" % (_col(ACCENT), line))
    weechat.prnt(
        buf,
        "%s  weechat-radio:%s$"
        % (_col(ACCENT), _col(ORANGE)),
    )
    weechat.prnt(
        buf,
        "%s  Type a message and press Enter.  /radio help for station commands."
        % _col(DIM),
    )
    weechat.prnt(buf, "")


def welcome_cb(_data, _remaining):
    _clear_buffer("core", "weechat")
    _clear_buffer("irc", "server.radio")
    _clear_buffer("irc", "radio")
    weechat.command("", "/buffer irc.radio.#bulletin")
    buf = weechat.buffer_search("irc", "radio.#bulletin")
    if not buf:
        buf = weechat.current_buffer()
    _print_banner(buf)
    return weechat.WEECHAT_RC_OK


def connected_cb(_data, _signal, _signal_data):
    weechat.hook_timer(250, 0, 1, "welcome_cb", "")
    return weechat.WEECHAT_RC_OK


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
    weechat.hook_signal("irc_server_connected", "connected_cb", "")
    if not weechat.bar_search("radio_bar"):
        _mute("/bar add radio_bar window bottom 1 0 radio,radio_air")
    apply_chrome()
    weechat.config_set_plugin("draft_len", "0")
    weechat.hook_timer(800, 0, 1, "welcome_cb", "")
