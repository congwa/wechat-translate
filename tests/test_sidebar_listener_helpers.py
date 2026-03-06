import importlib.util
import pathlib
import unittest
from urllib import error


def _load_sidebar_module():
    repo_root = pathlib.Path(__file__).resolve().parents[1]
    script_path = repo_root / "examples" / "sidebar_translate_listener.py"
    spec = importlib.util.spec_from_file_location("sidebar_translate_listener", script_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load module from {script_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


sidebar = _load_sidebar_module()


class SidebarHelpersTest(unittest.TestCase):
    def test_normalize_message_for_dedupe(self):
        raw = "  hello\u200b   world  "
        self.assertEqual(sidebar.normalize_message_for_dedupe(raw), "hello world")

    def test_validate_positive_float(self):
        self.assertEqual(sidebar.validate_positive_float("x", 0.5), 0.5)
        with self.assertRaises(RuntimeError):
            sidebar.validate_positive_float("x", 0)
        with self.assertRaises(RuntimeError):
            sidebar.validate_positive_float("x", -1)

    def test_validate_float_min(self):
        self.assertEqual(sidebar.validate_float_min("x", 0.6, 0.2), 0.6)
        with self.assertRaises(RuntimeError):
            sidebar.validate_float_min("x", 0.1, 0.2)

    def test_validate_int_min(self):
        self.assertEqual(sidebar.validate_int_min("w", 300, 280), 300)
        with self.assertRaises(RuntimeError):
            sidebar.validate_int_min("w", 200, 280)

    def test_read_config_type_validation(self):
        self.assertEqual(sidebar.read_config_float({}, "interval_seconds", 1.0), 1.0)
        self.assertEqual(sidebar.read_config_int({}, "width", 420), 420)

        with self.assertRaises(RuntimeError):
            sidebar.read_config_float({"interval_seconds": "bad"}, "interval_seconds", 1.0)
        with self.assertRaises(RuntimeError):
            sidebar.read_config_int({"width": "bad"}, "width", 420)

    def test_normalize_translate_provider_rejects_invalid_value(self):
        self.assertEqual(sidebar.normalize_translate_provider("DEEPLX"), "deeplx")
        with self.assertRaises(RuntimeError):
            sidebar.normalize_translate_provider("unknown")

    def test_validate_translate_config_requires_deeplx_url_when_enabled(self):
        with self.assertRaises(RuntimeError):
            sidebar.validate_translate_config(True, "deeplx", "")
        sidebar.validate_translate_config(False, "deeplx", "")
        sidebar.validate_translate_config(True, "passthrough", "")

    def test_create_translator_rejects_missing_deeplx_url(self):
        with self.assertRaises(RuntimeError):
            sidebar.create_translator(
                enabled=True,
                provider="deeplx",
                deeplx_url="",
                source_lang="auto",
                target_lang="EN",
                timeout_seconds=8.0,
            )

    def test_deeplx_translator_retries_urlerror_then_succeeds(self):
        class FakeResponse:
            def __enter__(self):
                return self

            def __exit__(self, exc_type, exc, tb):
                return False

            def read(self):
                return b'{"data":"hello"}'

        call_count = 0
        sleep_calls = []
        original_urlopen = sidebar.request.urlopen
        original_sleep = sidebar.time.sleep

        def fake_urlopen(req, timeout):
            nonlocal call_count
            call_count += 1
            if call_count < 3:
                raise error.URLError("timeout")
            return FakeResponse()

        sidebar.request.urlopen = fake_urlopen
        sidebar.time.sleep = sleep_calls.append
        try:
            translator = sidebar.DeepLXTranslator("http://127.0.0.1:1188/translate")
            result = translator.translate("你好")
        finally:
            sidebar.request.urlopen = original_urlopen
            sidebar.time.sleep = original_sleep

        self.assertEqual(result, "hello")
        self.assertEqual(call_count, 3)
        self.assertEqual(
            sleep_calls,
            [
                sidebar.DEEPLX_RETRY_BACKOFF_SECONDS,
                sidebar.DEEPLX_RETRY_BACKOFF_SECONDS * 2,
            ],
        )

    def test_append_message_with_limit(self):
        cache = []
        for idx in range(5):
            msg = sidebar.SidebarMessage(
                chat_name="g1",
                sender_name="u",
                text_en=f"m{idx}",
                created_at="10:00:00",
                is_self=False,
            )
            sidebar.append_message_with_limit(cache, msg, limit=3)
        self.assertEqual(len(cache), 3)
        self.assertEqual([item.text_en for item in cache], ["m2", "m3", "m4"])

    def test_replace_message_in_cache_matches_message_id(self):
        cache = [
            sidebar.SidebarMessage(
                chat_name="g1",
                sender_name="u",
                text_en="Loading...",
                created_at="10:00:00",
                is_self=False,
                message_id="msg-1",
                pending_translation=True,
            )
        ]
        replacement = sidebar.SidebarMessage(
            chat_name="g1",
            sender_name="u",
            text_en="hello",
            created_at="10:00:00",
            is_self=False,
            message_id="msg-1",
            pending_translation=False,
        )

        replaced = sidebar.replace_message_in_cache(cache, replacement)

        self.assertTrue(replaced)
        self.assertEqual(cache[0].text_en, "hello")
        self.assertFalse(cache[0].pending_translation)

    def test_append_message_active_chat_rerenders_from_cache(self):
        ui = object.__new__(sidebar.SidebarUI)
        ui.chat_messages = {"g1": []}
        ui.unread_counts = {"g1": 0}
        ui.chat_order = ["g1"]
        ui.active_chat = "g1"
        ui.message_limit = 2
        ui._ensure_chat = lambda chat_name: None
        ui._refresh_target_list = lambda: None

        render_calls = []
        insert_calls = []
        ui._render_active_chat = lambda: render_calls.append(
            [item.text_en for item in ui.chat_messages["g1"]]
        )
        ui._insert_message = lambda msg: insert_calls.append(msg.text_en)

        for idx in range(3):
            msg = sidebar.SidebarMessage(
                chat_name="g1",
                sender_name="u",
                text_en=f"m{idx}",
                created_at="10:00:00",
                is_self=False,
            )
            sidebar.SidebarUI.append_message(ui, msg)

        self.assertEqual(render_calls[-1], ["m1", "m2"])
        self.assertEqual(insert_calls, [])

    def test_replace_message_active_chat_rerenders_without_new_unread(self):
        ui = object.__new__(sidebar.SidebarUI)
        ui.chat_messages = {
            "g1": [
                sidebar.SidebarMessage(
                    chat_name="g1",
                    sender_name="u",
                    text_en="Loading...",
                    created_at="10:00:00",
                    is_self=False,
                    message_id="msg-1",
                    pending_translation=True,
                )
            ]
        }
        ui.unread_counts = {"g1": 1}
        ui.active_chat = "g1"
        ui._ensure_chat = lambda chat_name: None
        calls = []
        ui._update_active_chat_summary = lambda: calls.append("summary")
        ui._render_active_chat = lambda: calls.append("render")

        replaced = sidebar.SidebarUI.replace_message(
            ui,
            sidebar.SidebarMessage(
                chat_name="g1",
                sender_name="u",
                text_en="hello",
                created_at="10:00:00",
                is_self=False,
                message_id="msg-1",
                pending_translation=False,
            ),
        )

        self.assertTrue(replaced)
        self.assertEqual(ui.chat_messages["g1"][0].text_en, "hello")
        self.assertEqual(ui.unread_counts["g1"], 1)
        self.assertEqual(calls, ["summary", "render"])

    def test_build_worker_status_text(self):
        text = sidebar.build_worker_status_text(
            "waiting_wechat",
            "wechat not ready, retry in 10.0s",
            3,
        )
        self.assertEqual(text, "等待微信")
        self.assertEqual(sidebar.build_worker_status_text("running", "", 3), "")

    def test_compute_worker_restart_delay_caps(self):
        self.assertEqual(sidebar.compute_worker_restart_delay(1), 3.0)
        self.assertEqual(sidebar.compute_worker_restart_delay(2), 6.0)
        self.assertEqual(sidebar.compute_worker_restart_delay(3), 12.0)
        self.assertEqual(sidebar.compute_worker_restart_delay(4), 24.0)
        self.assertEqual(sidebar.compute_worker_restart_delay(5), 30.0)
        self.assertEqual(sidebar.compute_worker_restart_delay(9), 30.0)

    def test_truncate_target_label(self):
        self.assertEqual(sidebar.truncate_target_label("测试群123"), "测试群123")
        self.assertEqual(sidebar.truncate_target_label("测试群1234"), "测试群1234")
        self.assertEqual(sidebar.truncate_target_label("测试群12345"), "测试群12345")
        self.assertEqual(sidebar.truncate_target_label("测试群123456"), "测试群12345...")
        self.assertEqual(sidebar.truncate_target_label("ABCDEFGHI"), "ABCDEFGH...")

    def test_message_wraplength_and_side_gap_shrink_with_narrow_canvas(self):
        class FakeText:
            def __init__(self, width):
                self._width = width

            def winfo_width(self):
                return self._width

        class FakeRoot:
            def __init__(self, width):
                self._width = width

            def winfo_width(self):
                return self._width

        ui = object.__new__(sidebar.SidebarUI)
        ui.text = FakeText(150)
        ui.root = FakeRoot(200)

        gap = sidebar.SidebarUI._message_side_gap(ui)
        wrap = sidebar.SidebarUI._message_wraplength(ui)

        self.assertGreaterEqual(gap, sidebar.MIN_MESSAGE_SIDE_GAP)
        self.assertLessEqual(gap, sidebar.MAX_MESSAGE_SIDE_GAP)
        self.assertGreaterEqual(wrap, sidebar.MIN_MESSAGE_WRAP_WIDTH)
        self.assertLessEqual(wrap, sidebar.MAX_MESSAGE_WRAP_WIDTH)
        self.assertLess(wrap, 150)

    def test_toggle_target_panel_shortcut_returns_break(self):
        ui = object.__new__(sidebar.SidebarUI)
        calls = []
        ui.toggle_target_panel = lambda: calls.append("toggle")

        result = sidebar.SidebarUI.on_toggle_target_panel_shortcut(ui)

        self.assertEqual(calls, ["toggle"])
        self.assertEqual(result, "break")

    def test_cycle_chat_wraps_and_switches_active_chat(self):
        ui = object.__new__(sidebar.SidebarUI)
        ui.chat_order = ["群1", "群2", "群3"]
        ui.active_chat = "群2"
        switched = []
        ui.switch_chat = lambda name: switched.append(name)

        sidebar.SidebarUI.cycle_chat(ui, 1)
        sidebar.SidebarUI.cycle_chat(ui, -1)

        self.assertEqual(switched, ["群3", "群1"])

    def test_cycle_chat_uses_first_chat_when_active_missing(self):
        ui = object.__new__(sidebar.SidebarUI)
        ui.chat_order = ["群1", "群2"]
        ui.active_chat = ""
        switched = []
        ui.switch_chat = lambda name: switched.append(name)

        sidebar.SidebarUI.cycle_chat(ui, 1)

        self.assertEqual(switched, ["群2"])

    def test_switch_chat_shortcuts_return_break(self):
        ui = object.__new__(sidebar.SidebarUI)
        calls = []
        ui._handle_chat_switch_shortcut = lambda step: calls.append(step)

        prev_result = sidebar.SidebarUI.on_switch_prev_chat_shortcut(ui)
        next_result = sidebar.SidebarUI.on_switch_next_chat_shortcut(ui)

        self.assertEqual(calls, [-1, 1])
        self.assertEqual(prev_result, "break")
        self.assertEqual(next_result, "break")

    def test_handle_chat_switch_shortcut_debounces_fast_repeat(self):
        ui = object.__new__(sidebar.SidebarUI)
        ui._last_chat_switch_shortcut_at = 0.0
        calls = []
        ui.cycle_chat = lambda step: calls.append(step)

        original_monotonic = sidebar.time.monotonic
        times = iter([1.0, 1.05, 1.30])
        sidebar.time.monotonic = lambda: next(times)
        try:
            first = sidebar.SidebarUI._handle_chat_switch_shortcut(ui, 1)
            second = sidebar.SidebarUI._handle_chat_switch_shortcut(ui, 1)
            third = sidebar.SidebarUI._handle_chat_switch_shortcut(ui, -1)
        finally:
            sidebar.time.monotonic = original_monotonic

        self.assertTrue(first)
        self.assertFalse(second)
        self.assertTrue(third)
        self.assertEqual(calls, [1, -1])


if __name__ == "__main__":
    unittest.main()
