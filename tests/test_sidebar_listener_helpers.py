import io
import importlib.util
import os
import pathlib
import tempfile
import unittest
from unittest import mock
from urllib import error


def _load_sidebar_module():
    repo_root = pathlib.Path(__file__).resolve().parents[1]
    script_path = repo_root / "listener_app" / "sidebar_translate_listener.py"
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

    def test_is_filtered_placeholder_matches_media_placeholders(self):
        self.assertTrue(sidebar.is_filtered_placeholder("[图片]"))
        self.assertTrue(sidebar.is_filtered_placeholder("[动画表情]"))
        self.assertTrue(sidebar.is_filtered_placeholder('[语音] 2"'))
        self.assertTrue(sidebar.is_filtered_placeholder('[Voice Over] 3"'))
        self.assertFalse(sidebar.is_filtered_placeholder("咳"))

    def test_exit_startup_error_prints_and_raises(self):
        original_dialog = sidebar.maybe_show_frozen_error_dialog
        calls = []

        try:
            sidebar.maybe_show_frozen_error_dialog = calls.append
            with self.assertRaises(SystemExit) as cm, mock.patch(
                "sys.stderr",
                new_callable=io.StringIO,
            ) as stderr:
                sidebar.exit_startup_error("invalid config: missing deeplx url", exit_code=3)
        finally:
            sidebar.maybe_show_frozen_error_dialog = original_dialog

        self.assertEqual(cm.exception.code, 3)
        self.assertEqual(calls, ["invalid config: missing deeplx url"])
        self.assertIn(
            "[sidebar] invalid config: missing deeplx url",
            stderr.getvalue(),
        )

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

    def test_ensure_runtime_layout_copies_default_config(self):
        with tempfile.TemporaryDirectory() as runtime_dir, tempfile.TemporaryDirectory() as bundle_dir:
            bundled_config_dir = pathlib.Path(bundle_dir) / "config"
            bundled_config_dir.mkdir(parents=True, exist_ok=True)
            bundled_config_path = bundled_config_dir / "listener.json"
            bundled_config_path.write_text('{"listen": {}}', encoding="utf-8")

            runtime_config_path = sidebar.ensure_runtime_layout(
                runtime_root=runtime_dir,
                bundled_config_path=str(bundled_config_path),
            )

            self.assertTrue(os.path.exists(runtime_config_path))
            self.assertEqual(pathlib.Path(runtime_config_path).read_text(encoding="utf-8"), '{"listen": {}}')
            self.assertTrue(os.path.isdir(pathlib.Path(runtime_dir) / "logs"))

    def test_build_worker_command_uses_python_script_in_source_mode(self):
        cmd = sidebar.build_worker_command(
            ["群1", "群2"],
            0.6,
            debug=True,
            focus_refresh=True,
            load_retry_seconds=10.0,
            frozen=False,
            python_executable="python",
            source_root=r"D:\repo\wechat-pc-auto",
            runtime_root=r"D:\runtime",
        )

        self.assertEqual(cmd[:5], ["python", "-X", "utf8", "-u", r"D:\repo\wechat-pc-auto\listener_app\group_listener_worker.py"])
        self.assertIn("--targets-json", cmd)
        self.assertIn("--debug", cmd)
        self.assertIn("--focus-refresh", cmd)

    def test_build_worker_command_uses_worker_exe_in_frozen_mode(self):
        cmd = sidebar.build_worker_command(
            ["群1"],
            0.6,
            debug=False,
            focus_refresh=False,
            load_retry_seconds=10.0,
            frozen=True,
            python_executable="python",
            source_root=r"D:\repo\wechat-pc-auto",
            runtime_root=r"D:\runtime",
        )

        self.assertEqual(cmd[0], r"D:\runtime\group_listener_worker.exe")
        self.assertNotIn("-X", cmd)
        self.assertIn("--targets-json", cmd)

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
        ui._ensure_chat = lambda chat_name: True
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
        ui._ensure_chat = lambda chat_name: True
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
        self.assertEqual(calls, ["render"])

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
        self.assertEqual(sidebar.truncate_target_label("测试群1234"), "测试群123...")
        self.assertEqual(sidebar.truncate_target_label("ABCDEFG"), "ABCDEF...")

    def test_toggle_target_panel_shortcut_returns_break(self):
        ui = object.__new__(sidebar.SidebarUI)
        calls = []
        ui.toggle_target_panel = lambda: calls.append("toggle")

        result = sidebar.SidebarUI.on_toggle_target_panel_shortcut(ui)

        self.assertEqual(calls, ["toggle"])
        self.assertEqual(result, "break")

    def test_append_message_ignores_unknown_chat_when_targets_fixed(self):
        ui = object.__new__(sidebar.SidebarUI)
        ui.allowed_chat_names = {"g1"}
        ui.chat_messages = {"g1": []}
        ui.unread_counts = {"g1": 0}
        ui.chat_order = ["g1"]
        ui.active_chat = "g1"
        ui.message_limit = 2
        ui._refresh_target_list = lambda: None
        ui._render_active_chat = lambda: (_ for _ in ()).throw(AssertionError("should not render"))

        sidebar.SidebarUI.append_message(
            ui,
            sidebar.SidebarMessage(
                chat_name="unexpected",
                sender_name="u",
                text_en="hello",
                created_at="10:00:00",
                is_self=False,
            ),
        )

        self.assertEqual(ui.chat_order, ["g1"])
        self.assertEqual(ui.chat_messages["g1"], [])

    def test_insert_message_content_uses_original_text_when_toggle_enabled(self):
        class FakeVar:
            def __init__(self, value):
                self.value = value

            def get(self):
                return self.value

        class FakeText:
            def __init__(self):
                self.calls = []

            def insert(self, *_args):
                self.calls.append(_args)

        ui = object.__new__(sidebar.SidebarUI)
        ui.show_original_var = FakeVar(True)
        ui.text = FakeText()

        sidebar.SidebarUI._insert_message_content(
            ui,
            sidebar.SidebarMessage(
                chat_name="g1",
                sender_name="高钰",
                text_en="Ahem.",
                text_cn="咳",
                created_at="18:04:41",
                is_self=False,
                pending_translation=True,
            ),
        )

        self.assertEqual(ui.text.calls[1][1], "咳\n")
        self.assertEqual(ui.text.calls[1][2], "msg_left")


if __name__ == "__main__":
    unittest.main()
