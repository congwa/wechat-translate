import argparse
import asyncio
import atexit
import base64
import ctypes
import hashlib
import importlib
import json
import math
import os
import queue
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
import time
import tkinter as tk
import uuid
from dataclasses import dataclass
from datetime import datetime
from tkinter import font as tkfont
from tkinter import messagebox, scrolledtext, simpledialog, ttk
from urllib import error, request
from urllib.parse import urlparse
from typing import Any, Callable, Dict

# 侧边栏主进程：
# - 读取 JSON 配置
# - 启动 worker 监听目标会话
# - 处理消息事件并执行翻译
# - 以“聊天气泡”风格展示（支持自己消息右对齐）
SOURCE_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def is_frozen_app() -> bool:
    return bool(getattr(sys, "frozen", False))


def get_bundle_root() -> str:
    return str(getattr(sys, "_MEIPASS", SOURCE_ROOT))


def get_runtime_root() -> str:
    if is_frozen_app():
        return os.path.dirname(os.path.abspath(sys.executable))
    return SOURCE_ROOT


ROOT_DIR = get_runtime_root()
BUNDLE_ROOT = get_bundle_root()
DEFAULT_CONFIG_PATH = os.path.join(ROOT_DIR, "config", "listener.json")
BUNDLED_CONFIG_PATH = os.path.join(BUNDLE_ROOT, "config", "listener.json")
WORKER_EXE_NAME = "group_listener_worker.exe"

# 匹配 “发送人: 正文” / “发送人：正文”
SENDER_PREFIX_RE = re.compile(r"^\s*([^:：]{1,40})[:：]\s*(.+?)\s*$")
# 过滤图片/媒体占位文本（例如 [图片] / [视频] / [Images]）
IMAGE_PLACEHOLDER_RE = re.compile(r"^\[\s*(图片|image|images|photo)\s*\]$", re.IGNORECASE)
VIDEO_PLACEHOLDER_RE = re.compile(r"^\[\s*(视频|video)\s*\]$", re.IGNORECASE)
ANIMATED_EMOTICON_PLACEHOLDER_RE = re.compile(
    r"^\[\s*(动画表情|animated emoticon|emoticon|emoji|sticker)\s*\]$",
    re.IGNORECASE,
)
VOICE_PLACEHOLDER_RE = re.compile(
    r'^\[\s*(语音|voice(?:\s+over)?|audio)\s*\]\s*\d+\s*"*$',
    re.IGNORECASE,
)
GENERIC_BRACKET_PLACEHOLDER_RE = re.compile(r"^\[[^\r\n]+\]$")
HTTP_LINK_RE = re.compile(r"https?://", re.IGNORECASE)
PREFERRED_UI_FONTS = ("Cascadia Code", "JetBrains Mono", "黑体")
PREFERRED_ENGLISH_TTS_VOICES = ("Microsoft Zira Desktop", "Microsoft David Desktop")
DEFAULT_SIDEBAR_HEIGHT = 550
DEFAULT_META_FONT_SIZE = 10
MESSAGE_FONT_EXTRA_PX = 2
ZERO_WIDTH_RE = re.compile(r"[\u200b\u200c\u200d\ufeff]")
MULTI_SPACE_RE = re.compile(r"\s+")
ASCII_LETTER_RE = re.compile(r"[A-Za-z]")
CJK_TEXT_RE = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff]")
SESSION_PREVIEW_DEDUPE_WINDOW_SECONDS = 20.0
DEDUPE_CACHE_TTL_SECONDS = 600.0
DEDUPE_CACHE_MAX_KEYS = 5000
DEDUPE_CLEANUP_INTERVAL_SECONDS = 30.0
LOG_ROTATE_MAX_BYTES = 10 * 1024 * 1024
LOG_ROTATE_KEEP_FILES = 5
TRANSLATE_QUEUE_MAXSIZE = 300
TRANSLATE_QUEUE_DROP_LOG_INTERVAL_SECONDS = 5.0
DEEPLX_MAX_RETRIES = 2
DEEPLX_RETRY_BACKOFF_SECONDS = 0.6
MIN_SIDEBAR_WIDTH = 280
DEFAULT_SIDEBAR_WIDTH = 470
DEFAULT_TARGET_PANEL_WIDTH = 150
MIN_LISTEN_INTERVAL_SECONDS = 0.2
CHAT_CACHE_LIMIT = 100
TARGET_LABEL_MAX_CHARS = 6
DEFAULT_LISTEN_INTERVAL_SECONDS = 0.6
UI_DRAIN_INTERVAL_MS = 80
TRANSLATE_PENDING_TEXT = "Loading..."
META_TEXT_COLOR = "#555555"
TTS_BODY_CLICK_MOVE_TOLERANCE_PX = 4
CHAT_SWITCH_SHORTCUT_DEBOUNCE_SECONDS = 0.15
WORKER_RESTART_INITIAL_BACKOFF_SECONDS = 3.0
WORKER_RESTART_MAX_BACKOFF_SECONDS = 30.0
WORKER_STOP_TIMEOUT_SECONDS = 3.0
WORKER_FORCE_KILL_TIMEOUT_SECONDS = 1.0
RUNTIME_LOCK_DIR = os.path.join(ROOT_DIR, "logs", ".runtime")
SUPPORTED_TRANSLATE_PROVIDERS = ("deeplx", "passthrough")
SUPPORTED_TTS_PROVIDERS = ("windows_system", "doubao", "tencent_cloud")
DEFAULT_TTS_PROVIDER = "tencent_cloud"
DOUBAO_TTS_DEFAULT_CONFIG_PATH = os.path.join("config", "doubao_tts.json")
DOUBAO_TTS_DEFAULT_ENDPOINT = "wss://openspeech.bytedance.com/api/v3/tts/unidirectional/stream"
DOUBAO_TTS_SUPPORTED_SAMPLE_RATES = (8000, 16000, 22050, 24000, 32000, 44100, 48000)
DOUBAO_TTS_DEFAULT_SAMPLE_RATE = 32000
DOUBAO_TTS_DEFAULT_SPEECH_RATE = -15
DOUBAO_TTS_DEFAULT_LOUDNESS_RATE = 0
TENCENT_CLOUD_TTS_DEFAULT_CONFIG_PATH = os.path.join("config", "tencent_tts.json")
TENCENT_CLOUD_TTS_DEFAULT_ENDPOINT = "tts.tencentcloudapi.com"
TENCENT_CLOUD_TTS_SUPPORTED_SAMPLE_RATES = (8000, 16000, 24000)
TENCENT_CLOUD_TTS_SUPPORTED_PRIMARY_LANGUAGES = (1, 2)
TENCENT_CLOUD_TTS_SUPPORTED_SEGMENT_RATES = (0, 1, 2)
TENCENT_CLOUD_TTS_DEFAULT_SAMPLE_RATE = 16000
TENCENT_CLOUD_TTS_DEFAULT_SPEED = 0.0
TENCENT_CLOUD_TTS_DEFAULT_VOLUME = 0.0
TENCENT_CLOUD_TTS_DEFAULT_MODEL_TYPE = 1
TENCENT_CLOUD_TTS_DEFAULT_PROJECT_ID = 0
TENCENT_CLOUD_TTS_DEFAULT_PRIMARY_LANGUAGE = 2
TENCENT_CLOUD_TTS_DEFAULT_SEGMENT_RATE = 0
DOUBAO_HEADER_FIXED = bytes([0x11, 0x10, 0x10, 0x00])
DOUBAO_MSG_TYPE_FULL_SERVER_RESPONSE = 0x9
DOUBAO_MSG_TYPE_AUDIO_ONLY_SERVER = 0xB
DOUBAO_MSG_TYPE_ERROR = 0xF
DOUBAO_FLAG_POSITIVE_SEQ = 0x1
DOUBAO_FLAG_NEGATIVE_SEQ = 0x3
DOUBAO_FLAG_WITH_EVENT = 0x4
DOUBAO_EVENT_CONNECTION_STARTED = 50
DOUBAO_EVENT_CONNECTION_FAILED = 51
DOUBAO_EVENT_CONNECTION_FINISHED = 52
DOUBAO_EVENT_SESSION_FINISHED = 152
DOUBAO_EVENT_SESSION_FAILED = 153
_LOG_WRITE_LOCK = threading.Lock()
_TARGET_LOCK_PATHS: list[str] = []


def get_system_double_click_time_ms(default: int = 500) -> int:
    if os.name != "nt":
        return default
    try:
        value = int(ctypes.windll.user32.GetDoubleClickTime())
    except Exception:
        return default
    return max(200, min(1000, value))


TTS_BODY_CLICK_PLAY_DELAY_MS = get_system_double_click_time_ms()


def load_local_env():
    env_path = os.path.join(ROOT_DIR, ".env.local")
    if not os.path.exists(env_path):
        return
    try:
        with open(env_path, "r", encoding="utf-8") as f:
            for raw in f:
                line = raw.strip()
                if not line or line.startswith("#") or "=" not in line:
                    continue
                key, value = line.split("=", 1)
                key = key.strip()
                value = value.strip().strip('"').strip("'")
                if key and key not in os.environ:
                    os.environ[key] = value
    except Exception:
        pass


def ensure_runtime_layout(
    runtime_root: str = ROOT_DIR,
    bundled_config_path: str = BUNDLED_CONFIG_PATH,
) -> str:
    config_dir = os.path.join(runtime_root, "config")
    logs_dir = os.path.join(runtime_root, "logs")
    os.makedirs(config_dir, exist_ok=True)
    os.makedirs(logs_dir, exist_ok=True)

    runtime_config_path = os.path.join(config_dir, "listener.json")
    bundled_config = str(bundled_config_path or "").strip()
    bundled_config_dir = os.path.dirname(bundled_config) if bundled_config else ""
    if bundled_config_dir and os.path.isdir(bundled_config_dir):
        for name in os.listdir(bundled_config_dir):
            if not str(name).lower().endswith(".json"):
                continue
            source_path = os.path.join(bundled_config_dir, name)
            target_path = os.path.join(config_dir, name)
            if os.path.isfile(source_path) and not os.path.exists(target_path):
                shutil.copy2(source_path, target_path)
    return runtime_config_path


ensure_runtime_layout()
load_local_env()


def load_json_config(path: str) -> Dict[str, Any]:
    # 仅接受 object 根节点，避免把数组/字符串误当配置。
    with open(path, "r", encoding="utf-8") as f:
        raw = json.load(f)
    if not isinstance(raw, dict):
        raise RuntimeError(f"config root must be object: {path}")
    return raw


def save_json_config_atomic(path: str, payload: Dict[str, Any]):
    normalized = os.path.abspath(path)
    parent = os.path.dirname(normalized)
    if parent:
        os.makedirs(parent, exist_ok=True)

    fd, temp_path = tempfile.mkstemp(
        prefix="listener.",
        suffix=".tmp",
        dir=parent or None,
    )
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as f:
            json.dump(payload, f, ensure_ascii=False, indent=2)
            f.write("\n")
            f.flush()
            os.fsync(f.fileno())
        os.replace(temp_path, normalized)
    except Exception:
        try:
            if os.path.exists(temp_path):
                os.remove(temp_path)
        except Exception:
            pass
        raise


def save_listener_targets_config(path: str, config: Dict[str, Any], targets: list[str]):
    normalized_targets = normalize_targets(targets)
    if not normalized_targets:
        raise RuntimeError("listen.targets cannot be empty")

    payload = dict(config)
    listen_cfg = payload.get("listen", {})
    if not isinstance(listen_cfg, dict):
        listen_cfg = {}
    else:
        listen_cfg = dict(listen_cfg)
    listen_cfg["targets"] = normalized_targets
    payload["listen"] = listen_cfg

    save_json_config_atomic(path, payload)
    config.clear()
    config.update(payload)


def resolve_config_file_path(path: Any, *, base_dir: str = ROOT_DIR) -> str:
    raw = str(path or "").strip()
    if not raw:
        return ""
    if os.path.isabs(raw):
        return os.path.normpath(raw)
    candidates: list[str] = []
    if base_dir:
        candidates.append(os.path.join(base_dir, raw))
    candidates.append(os.path.join(ROOT_DIR, raw))
    for candidate in candidates:
        normalized = os.path.normpath(candidate)
        if os.path.exists(normalized):
            return normalized
    return os.path.normpath(candidates[0])


def read_secret_config_value(cfg: dict[str, Any], key: str, env_key: str) -> str:
    value = str(cfg.get(key) or "").strip()
    if value:
        return value
    env_name = str(cfg.get(env_key) or "").strip()
    if env_name:
        return str(os.getenv(env_name, "")).strip()
    return ""


def as_bool(value: Any, default: bool = False) -> bool:
    if isinstance(value, bool):
        return value
    if value is None:
        return default
    if isinstance(value, str):
        v = value.strip().lower()
        if v in ("1", "true", "yes", "on"):
            return True
        if v in ("0", "false", "no", "off"):
            return False
    return default


def as_float(value: Any, default: float) -> float:
    try:
        return float(value)
    except Exception:
        return default


def as_non_negative_float(value: Any, default: float) -> float:
    parsed = as_float(value, default)
    if not math.isfinite(parsed) or parsed < 0:
        return default
    return parsed


def as_int(value: Any, default: int) -> int:
    try:
        return int(value)
    except Exception:
        return default


def normalize_tts_provider(value: Any) -> str:
    provider = str(value or DEFAULT_TTS_PROVIDER).strip().lower()
    provider = provider or DEFAULT_TTS_PROVIDER
    if provider not in SUPPORTED_TTS_PROVIDERS:
        raise RuntimeError(
            f"tts.provider must be one of {', '.join(SUPPORTED_TTS_PROVIDERS)}, got {value!r}"
        )
    return provider


def read_config_float(cfg: dict[str, Any], key: str, default: float) -> float:
    if key not in cfg:
        return default
    value = cfg.get(key)
    try:
        return float(value)
    except Exception as e:
        raise RuntimeError(f"{key} must be number, got {value!r}") from e


def read_config_int(cfg: dict[str, Any], key: str, default: int) -> int:
    if key not in cfg:
        return default
    value = cfg.get(key)
    try:
        return int(value)
    except Exception as e:
        raise RuntimeError(f"{key} must be integer, got {value!r}") from e


def validate_positive_float(name: str, value: float) -> float:
    if not math.isfinite(value) or value <= 0:
        raise RuntimeError(f"{name} must be > 0, got {value!r}")
    return value


def validate_float_min(name: str, value: float, minimum: float) -> float:
    if value < minimum:
        raise RuntimeError(f"{name} must be >= {minimum}, got {value!r}")
    return value


def validate_float_range(name: str, value: float, minimum: float, maximum: float) -> float:
    if value < minimum or value > maximum:
        raise RuntimeError(f"{name} must be in [{minimum}, {maximum}], got {value!r}")
    return value


def validate_int_min(name: str, value: int, minimum: int) -> int:
    if value < minimum:
        raise RuntimeError(f"{name} must be >= {minimum}, got {value!r}")
    return value


def validate_int_range(name: str, value: int, minimum: int, maximum: int) -> int:
    if value < minimum or value > maximum:
        raise RuntimeError(f"{name} must be in [{minimum}, {maximum}], got {value!r}")
    return value


def validate_int_choices(name: str, value: int, choices: tuple[int, ...]) -> int:
    if value not in choices:
        joined = ", ".join(str(item) for item in choices)
        raise RuntimeError(f"{name} must be one of [{joined}], got {value!r}")
    return value


def normalize_targets(value: Any) -> list[str]:
    # 归一化 targets：去空、去重、保序。
    if not isinstance(value, list):
        return []
    targets = []
    for item in value:
        name = str(item or "").strip()
        if name and name not in targets:
            targets.append(name)
    return targets


def split_sender_and_body(text: str) -> tuple[str, str, bool]:
    # 返回 (发送人, 正文, 是否视为自己发出)：
    # - 匹配到“发送人: 正文” -> 非自己消息
    # - 其余情况 -> 视为自己消息（右对齐显示）
    s = str(text or "").strip()
    if not s:
        return "", "", True
    if "://" in s:
        # URL 文本里常见冒号，不能误判为“发送人:正文”。
        return "", s, True
    m = SENDER_PREFIX_RE.match(s)
    if not m:
        return "", s, True
    sender = (m.group(1) or "").strip()
    body = (m.group(2) or "").strip()
    if sender.lower() in ("http", "https", "ftp") or sender.isdigit():
        # 防止把协议头或纯数字时间片段误识别为发送人。
        return "", s, True
    if not body:
        return "", s, True
    return sender, body, False


def is_image_placeholder(text: str) -> bool:
    return bool(IMAGE_PLACEHOLDER_RE.match(str(text or "").strip()))


def is_filtered_placeholder(text: str) -> bool:
    value = str(text or "").strip()
    if not value:
        return False
    return any(
        pattern.match(value)
        for pattern in (
            IMAGE_PLACEHOLDER_RE,
            VIDEO_PLACEHOLDER_RE,
            ANIMATED_EMOTICON_PLACEHOLDER_RE,
            VOICE_PLACEHOLDER_RE,
            GENERIC_BRACKET_PLACEHOLDER_RE,
        )
    )


def is_filtered_link_message(text: str) -> bool:
    value = str(text or "").strip()
    if not value:
        return False
    return bool(HTTP_LINK_RE.search(value))


def normalize_message_for_dedupe(text: str) -> str:
    cleaned = ZERO_WIDTH_RE.sub("", str(text or ""))
    cleaned = MULTI_SPACE_RE.sub(" ", cleaned).strip()
    return cleaned


def normalize_tts_text(text: str) -> str:
    return MULTI_SPACE_RE.sub(" ", str(text or "")).strip()


def summarize_tts_text(text: str, max_chars: int = 48) -> str:
    normalized = normalize_tts_text(text)
    if not normalized:
        return ""
    if len(normalized) <= max_chars:
        return normalized
    return normalized[:max_chars] + "..."


def is_speakable_english_text(text: str) -> bool:
    value = normalize_tts_text(text)
    if not value or value == TRANSLATE_PENDING_TEXT:
        return False
    if "translate_failed:" in value.lower():
        return False
    if CJK_TEXT_RE.search(value):
        return False
    return bool(ASCII_LETTER_RE.search(value))


def pick_preferred_tts_voice(voices: list[dict[str, str]]) -> str:
    if not voices:
        return ""
    by_name = {}
    english_names = []
    for item in voices:
        name = str(item.get("name") or "").strip()
        culture = str(item.get("culture") or "").strip().lower()
        if not name:
            continue
        by_name[name.lower()] = name
        if culture.startswith("en"):
            english_names.append(name)
    for preferred in PREFERRED_ENGLISH_TTS_VOICES:
        chosen = by_name.get(preferred.lower())
        if chosen:
            return chosen
    return english_names[0] if english_names else ""


def list_windows_tts_voices() -> list[dict[str, str]]:
    if os.name != "nt":
        return []
    command = (
        "$ErrorActionPreference='Stop';"
        "Add-Type -AssemblyName System.Speech;"
        "$s=New-Object System.Speech.Synthesis.SpeechSynthesizer;"
        "$s.GetInstalledVoices() | ForEach-Object {"
        "$v=$_.VoiceInfo;"
        "[Console]::Out.WriteLine(($v.Name)+'|'+($v.Culture.Name))"
        "}"
    )
    creationflags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    try:
        result = subprocess.run(
            [
                "powershell",
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                command,
            ],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=5,
            creationflags=creationflags,
            check=False,
        )
    except Exception:
        return []
    if result.returncode != 0:
        return []
    voices = []
    for raw in str(result.stdout or "").splitlines():
        line = raw.strip()
        if not line:
            continue
        parts = line.split("|", 1)
        name = parts[0].strip()
        culture = parts[1].strip() if len(parts) > 1 else ""
        if name:
            voices.append({"name": name, "culture": culture})
    return voices


def probe_python_module(module_name: str, *, required_attr: str = "") -> str:
    try:
        module = importlib.import_module(module_name)
    except Exception as e:
        return f"missing Python module '{module_name}': {e}"
    if required_attr:
        attr = getattr(module, required_attr, None)
        if not callable(attr):
            return f"Python module '{module_name}' missing callable '{required_attr}'"
    return ""


def probe_doubao_websocket_runtime() -> str:
    return probe_python_module("websockets", required_attr="connect")


def probe_tencent_cloud_tts_runtime() -> str:
    checks = (
        ("tencentcloud.common.credential", "Credential"),
        ("tencentcloud.common.profile.client_profile", "ClientProfile"),
        ("tencentcloud.common.profile.http_profile", "HttpProfile"),
        (
            "tencentcloud.common.exception.tencent_cloud_sdk_exception",
            "TencentCloudSDKException",
        ),
        ("tencentcloud.tts.v20190823.tts_client", "TtsClient"),
        ("tencentcloud.tts.v20190823.models", "TextToVoiceRequest"),
    )
    for module_name, attr_name in checks:
        error_text = probe_python_module(module_name, required_attr=attr_name)
        if error_text:
            return error_text
    return ""


def play_wav_bytes_on_windows(audio_data: bytes) -> bool:
    import winsound

    temp_path = ""
    try:
        with tempfile.NamedTemporaryFile(delete=False, suffix=".wav") as f:
            temp_path = f.name
            f.write(audio_data)
        winsound.PlaySound(temp_path, winsound.SND_FILENAME)
    finally:
        if temp_path:
            try:
                os.remove(temp_path)
            except OSError:
                pass
    return True


class WindowsSystemTTS:
    def __init__(self, voice_name: str = ""):
        self.voice_name = str(voice_name or "").strip()
        self._lock = threading.Lock()
        self._voice_probe_done = bool(self.voice_name)
        self._queue: "queue.SimpleQueue[str | None]" = queue.SimpleQueue()
        self._worker_started = False
        self._last_error = ""
        self._logger: Callable[[str], None] | None = None

    @classmethod
    def create_default(cls) -> "WindowsSystemTTS | None":
        if os.name != "nt":
            return None
        return cls()

    def _resolve_voice_name(self) -> str:
        with self._lock:
            if self._voice_probe_done:
                return self.voice_name
            self.voice_name = pick_preferred_tts_voice(list_windows_tts_voices())
            self._voice_probe_done = True
            if not self.voice_name:
                self._last_error = "no english system voice detected"
                self._emit_log("tts failed backend=windows_system reason=no english system voice detected")
            return self.voice_name

    def set_logger(self, logger: Callable[[str], None] | None):
        with self._lock:
            self._logger = logger

    def _emit_log(self, line: str):
        logger = self._logger
        if not logger:
            return
        try:
            logger(str(line or ""))
        except Exception:
            pass

    def _run_speak_blocking(self, payload: str) -> bool:
        voice_name = self._resolve_voice_name()
        if not voice_name:
            return False
        preview = summarize_tts_text(payload)
        self._emit_log(
            f"tts synthesize start backend=windows_system voice={voice_name} chars={len(payload)} preview={preview}"
        )
        command = (
            "$ErrorActionPreference='Stop';"
            "Add-Type -AssemblyName System.Speech;"
            "$text=[Console]::In.ReadToEnd();"
            "if([string]::IsNullOrWhiteSpace($text)){exit 1};"
            "$s=New-Object System.Speech.Synthesis.SpeechSynthesizer;"
            "$s.SelectVoice($env:WX_SIDEBAR_TTS_VOICE);"
            "$s.Volume=100;"
            "$s.Rate=0;"
            "$s.Speak($text);"
        )
        creationflags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
        env = os.environ.copy()
        env["WX_SIDEBAR_TTS_VOICE"] = voice_name
        try:
            result = subprocess.run(
                [
                    "powershell",
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    command,
                ],
                input=payload,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                text=True,
                encoding="utf-8",
                errors="replace",
                env=env,
                creationflags=creationflags,
                check=False,
            )
        except Exception as e:
            self._last_error = str(e)
            self._emit_log(f"tts failed backend=windows_system error={self._last_error}")
            return False
        if result.returncode != 0:
            self._last_error = f"tts process exited code={result.returncode}"
            self._emit_log(f"tts failed backend=windows_system error={self._last_error}")
            return False
        self._last_error = ""
        self._emit_log(
            f"tts played backend=windows_system voice={voice_name} chars={len(payload)} preview={preview}"
        )
        return True

    def _ensure_worker_started(self):
        with self._lock:
            if self._worker_started:
                return
            self._worker_started = True
        threading.Thread(target=self._worker_loop, daemon=True).start()

    def _worker_loop(self):
        while True:
            payload = self._queue.get()
            if payload is None:
                return
            self._run_speak_blocking(payload)

    def speak_async(self, text: str) -> bool:
        if os.name != "nt":
            self._emit_log("tts rejected backend=windows_system reason=non_windows")
            return False
        payload = normalize_tts_text(text)
        if not is_speakable_english_text(payload):
            self._emit_log(
                f"tts rejected backend=windows_system reason=non_speakable preview={summarize_tts_text(payload)}"
            )
            return False
        self._ensure_worker_started()
        self._queue.put(payload)
        self._emit_log(
            f"tts queued backend=windows_system chars={len(payload)} preview={summarize_tts_text(payload)}"
        )
        return True


@dataclass(frozen=True)
class DoubaoTTSSettings:
    endpoint: str
    appid: str
    access_token: str
    resource_id: str
    speaker: str
    audio_format: str = "wav"
    sample_rate: int = DOUBAO_TTS_DEFAULT_SAMPLE_RATE
    speech_rate: int = DOUBAO_TTS_DEFAULT_SPEECH_RATE
    loudness_rate: int = DOUBAO_TTS_DEFAULT_LOUDNESS_RATE
    use_cache: bool = False
    uid: str = "wechat-pc-auto"
    connect_timeout_seconds: float = 10.0


@dataclass(frozen=True)
class DoubaoWSMessage:
    msg_type: int
    flag: int
    event: int = 0
    error_code: int = 0
    payload: bytes = b""


@dataclass(frozen=True)
class TencentCloudTTSSettings:
    secret_id: str
    secret_key: str
    voice_type: int
    endpoint: str = TENCENT_CLOUD_TTS_DEFAULT_ENDPOINT
    region: str = ""
    codec: str = "wav"
    sample_rate: int = TENCENT_CLOUD_TTS_DEFAULT_SAMPLE_RATE
    speed: float = TENCENT_CLOUD_TTS_DEFAULT_SPEED
    volume: float = TENCENT_CLOUD_TTS_DEFAULT_VOLUME
    primary_language: int = TENCENT_CLOUD_TTS_DEFAULT_PRIMARY_LANGUAGE
    model_type: int = TENCENT_CLOUD_TTS_DEFAULT_MODEL_TYPE
    project_id: int = TENCENT_CLOUD_TTS_DEFAULT_PROJECT_ID
    segment_rate: int = TENCENT_CLOUD_TTS_DEFAULT_SEGMENT_RATE
    enable_subtitle: bool = False
    emotion_category: str = ""
    emotion_intensity: int = 100
    request_timeout_seconds: float = 15.0


def load_doubao_tts_settings(config_path: str, *, base_dir: str = ROOT_DIR) -> DoubaoTTSSettings:
    resolved_path = resolve_config_file_path(config_path, base_dir=base_dir)
    if not resolved_path or not os.path.isfile(resolved_path):
        raise RuntimeError(f"tts config not found: {config_path!r}")
    raw = load_json_config(resolved_path)
    provider = str(raw.get("provider") or "doubao").strip().lower()
    if provider and provider != "doubao":
        raise RuntimeError(f"doubao config provider must be 'doubao', got {provider!r}")

    endpoint = str(raw.get("endpoint") or DOUBAO_TTS_DEFAULT_ENDPOINT).strip()
    appid = read_secret_config_value(raw, "appid", "appid_env")
    access_token = read_secret_config_value(raw, "access_token", "access_token_env")
    resource_id = str(raw.get("resource_id") or "").strip()
    speaker = str(raw.get("speaker") or "").strip()
    audio_format = str(raw.get("audio_format") or "wav").strip().lower()
    sample_rate = read_config_int(raw, "sample_rate", DOUBAO_TTS_DEFAULT_SAMPLE_RATE)
    speech_rate = read_config_int(raw, "speech_rate", DOUBAO_TTS_DEFAULT_SPEECH_RATE)
    loudness_rate = read_config_int(raw, "loudness_rate", DOUBAO_TTS_DEFAULT_LOUDNESS_RATE)
    use_cache = as_bool(raw.get("use_cache"), False)
    uid = str(raw.get("uid") or "wechat-pc-auto").strip() or "wechat-pc-auto"
    connect_timeout_seconds = read_config_float(raw, "connect_timeout_seconds", 10.0)

    if not endpoint:
        raise RuntimeError("doubao endpoint is required")
    if not appid:
        raise RuntimeError("doubao appid/appid_env is required")
    if not access_token:
        raise RuntimeError("doubao access_token/access_token_env is required")
    if not resource_id:
        raise RuntimeError("doubao resource_id is required")
    if not speaker:
        raise RuntimeError("doubao speaker is required")
    if audio_format != "wav":
        raise RuntimeError(
            f"doubao audio_format must be 'wav' for current Windows playback path, got {audio_format!r}"
        )
    sample_rate = validate_int_choices(
        "doubao.sample_rate", sample_rate, DOUBAO_TTS_SUPPORTED_SAMPLE_RATES
    )
    speech_rate = validate_int_range("doubao.speech_rate", speech_rate, -50, 100)
    loudness_rate = validate_int_range("doubao.loudness_rate", loudness_rate, -50, 100)
    connect_timeout_seconds = validate_positive_float(
        "doubao.connect_timeout_seconds",
        connect_timeout_seconds,
    )
    return DoubaoTTSSettings(
        endpoint=endpoint,
        appid=appid,
        access_token=access_token,
        resource_id=resource_id,
        speaker=speaker,
        audio_format=audio_format,
        sample_rate=sample_rate,
        speech_rate=speech_rate,
        loudness_rate=loudness_rate,
        use_cache=use_cache,
        uid=uid,
        connect_timeout_seconds=connect_timeout_seconds,
    )


def load_tencent_cloud_tts_settings(
    config_path: str,
    *,
    base_dir: str = ROOT_DIR,
) -> TencentCloudTTSSettings:
    resolved_path = resolve_config_file_path(config_path, base_dir=base_dir)
    if not resolved_path or not os.path.isfile(resolved_path):
        raise RuntimeError(f"tts config not found: {config_path!r}")
    raw = load_json_config(resolved_path)
    provider = str(raw.get("provider") or "tencent_cloud").strip().lower()
    if provider and provider != "tencent_cloud":
        raise RuntimeError(
            f"tencent_cloud config provider must be 'tencent_cloud', got {provider!r}"
        )

    endpoint = str(raw.get("endpoint") or TENCENT_CLOUD_TTS_DEFAULT_ENDPOINT).strip()
    region = str(raw.get("region") or "").strip()
    secret_id = read_secret_config_value(raw, "secret_id", "secret_id_env")
    secret_key = read_secret_config_value(raw, "secret_key", "secret_key_env")
    voice_type = read_config_int(raw, "voice_type", 0)
    codec = str(raw.get("codec") or "wav").strip().lower()
    sample_rate = read_config_int(raw, "sample_rate", TENCENT_CLOUD_TTS_DEFAULT_SAMPLE_RATE)
    speed = read_config_float(raw, "speed", TENCENT_CLOUD_TTS_DEFAULT_SPEED)
    volume = read_config_float(raw, "volume", TENCENT_CLOUD_TTS_DEFAULT_VOLUME)
    primary_language = read_config_int(
        raw,
        "primary_language",
        TENCENT_CLOUD_TTS_DEFAULT_PRIMARY_LANGUAGE,
    )
    model_type = read_config_int(raw, "model_type", TENCENT_CLOUD_TTS_DEFAULT_MODEL_TYPE)
    project_id = read_config_int(raw, "project_id", TENCENT_CLOUD_TTS_DEFAULT_PROJECT_ID)
    segment_rate = read_config_int(raw, "segment_rate", TENCENT_CLOUD_TTS_DEFAULT_SEGMENT_RATE)
    enable_subtitle = as_bool(raw.get("enable_subtitle"), False)
    emotion_category = str(raw.get("emotion_category") or "").strip()
    emotion_intensity = read_config_int(raw, "emotion_intensity", 100)
    request_timeout_seconds = read_config_float(raw, "request_timeout_seconds", 15.0)

    if not endpoint:
        raise RuntimeError("tencent_cloud endpoint is required")
    if not secret_id:
        raise RuntimeError("tencent_cloud secret_id/secret_id_env is required")
    if not secret_key:
        raise RuntimeError("tencent_cloud secret_key/secret_key_env is required")
    voice_type = validate_int_min("tencent_cloud.voice_type", voice_type, 1)
    if codec != "wav":
        raise RuntimeError(
            f"tencent_cloud codec must be 'wav' for current Windows playback path, got {codec!r}"
        )
    sample_rate = validate_int_choices(
        "tencent_cloud.sample_rate",
        sample_rate,
        TENCENT_CLOUD_TTS_SUPPORTED_SAMPLE_RATES,
    )
    speed = validate_float_range("tencent_cloud.speed", speed, -2.0, 6.0)
    volume = validate_float_range("tencent_cloud.volume", volume, -10.0, 10.0)
    primary_language = validate_int_choices(
        "tencent_cloud.primary_language",
        primary_language,
        TENCENT_CLOUD_TTS_SUPPORTED_PRIMARY_LANGUAGES,
    )
    model_type = validate_int_choices("tencent_cloud.model_type", model_type, (1,))
    project_id = validate_int_min("tencent_cloud.project_id", project_id, 0)
    segment_rate = validate_int_choices(
        "tencent_cloud.segment_rate",
        segment_rate,
        TENCENT_CLOUD_TTS_SUPPORTED_SEGMENT_RATES,
    )
    if emotion_category:
        emotion_intensity = validate_int_range(
            "tencent_cloud.emotion_intensity",
            emotion_intensity,
            50,
            200,
        )
    request_timeout_seconds = validate_positive_float(
        "tencent_cloud.request_timeout_seconds",
        request_timeout_seconds,
    )
    return TencentCloudTTSSettings(
        secret_id=secret_id,
        secret_key=secret_key,
        voice_type=voice_type,
        endpoint=endpoint,
        region=region,
        codec=codec,
        sample_rate=sample_rate,
        speed=speed,
        volume=volume,
        primary_language=primary_language,
        model_type=model_type,
        project_id=project_id,
        segment_rate=segment_rate,
        enable_subtitle=enable_subtitle,
        emotion_category=emotion_category,
        emotion_intensity=emotion_intensity,
        request_timeout_seconds=request_timeout_seconds,
    )


def build_doubao_ws_headers(settings: DoubaoTTSSettings, connect_id: str) -> dict[str, str]:
    return {
        "X-Api-App-Key": settings.appid,
        "X-Api-Access-Key": settings.access_token,
        "X-Api-Resource-Id": settings.resource_id,
        "X-Api-Connect-Id": connect_id,
    }


def build_doubao_tts_request_payload(settings: DoubaoTTSSettings, text: str) -> dict[str, Any]:
    req_params: dict[str, Any] = {
        "text": text,
        "speaker": settings.speaker,
        "audio_params": {
            "format": settings.audio_format,
            "sample_rate": settings.sample_rate,
            "speech_rate": settings.speech_rate,
            "loudness_rate": settings.loudness_rate,
        },
    }
    if settings.use_cache:
        req_params["additions"] = {
            "cache_config": {
                "text_type": 1,
                "use_cache": True,
            }
        }
    return {
        "user": {
            "uid": settings.uid,
        },
        "req_params": req_params,
    }


def build_doubao_tts_runtime_fields(settings: DoubaoTTSSettings, config_path: str) -> str:
    return (
        f"backend=doubao config={config_path} "
        f"resource_id={settings.resource_id} speaker={settings.speaker} format={settings.audio_format} "
        f"sample_rate={settings.sample_rate} speech_rate={settings.speech_rate} "
        f"loudness_rate={settings.loudness_rate} use_cache={settings.use_cache}"
    )


def build_tencent_cloud_tts_request_payload(
    settings: TencentCloudTTSSettings,
    text: str,
    session_id: str,
) -> dict[str, Any]:
    payload: dict[str, Any] = {
        "Text": text,
        "SessionId": session_id,
        "Volume": settings.volume,
        "Speed": settings.speed,
        "ProjectId": settings.project_id,
        "ModelType": settings.model_type,
        "VoiceType": settings.voice_type,
        "PrimaryLanguage": settings.primary_language,
        "SampleRate": settings.sample_rate,
        "Codec": settings.codec,
        "EnableSubtitle": settings.enable_subtitle,
        "SegmentRate": settings.segment_rate,
    }
    if settings.emotion_category:
        payload["EmotionCategory"] = settings.emotion_category
        payload["EmotionIntensity"] = settings.emotion_intensity
    return payload


def build_tencent_cloud_tts_runtime_fields(
    settings: TencentCloudTTSSettings,
    config_path: str,
) -> str:
    region = settings.region or "-"
    fields = (
        f"backend=tencent_cloud config={config_path} endpoint={settings.endpoint} "
        f"region={region} voice_type={settings.voice_type} format={settings.codec} "
        f"sample_rate={settings.sample_rate} speed={settings.speed} volume={settings.volume} "
        f"primary_language={settings.primary_language} segment_rate={settings.segment_rate} "
        f"subtitle={settings.enable_subtitle}"
    )
    if settings.emotion_category:
        fields += (
            f" emotion_category={settings.emotion_category}"
            f" emotion_intensity={settings.emotion_intensity}"
        )
    return fields


def format_tencent_cloud_sdk_exception(exc: Exception) -> str:
    code = str(getattr(exc, "code", "") or "").strip()
    message = str(getattr(exc, "message", "") or str(exc)).strip()
    request_id = str(getattr(exc, "requestId", "") or "").strip()
    parts = []
    if code:
        parts.append(f"code={code}")
    if message:
        parts.append(f"message={message}")
    if request_id:
        parts.append(f"request_id={request_id}")
    return " ".join(parts) if parts else str(exc)


def build_doubao_full_client_request_frame(payload: bytes) -> bytes:
    return DOUBAO_HEADER_FIXED + struct.pack(">I", len(payload)) + payload


def find_wav_data_chunk_offset(audio_data: bytes) -> int:
    offset = 12
    total = len(audio_data)
    while offset + 8 <= total:
        chunk_id = audio_data[offset : offset + 4]
        chunk_size = struct.unpack("<I", audio_data[offset + 4 : offset + 8])[0]
        if chunk_id == b"data":
            return offset
        if chunk_size == 0xFFFFFFFF:
            break
        next_offset = offset + 8 + chunk_size + (chunk_size & 1)
        if next_offset <= offset or next_offset > total:
            break
        offset = next_offset
    return audio_data.find(b"data", 12)


def normalize_wav_size_fields(audio_data: bytes) -> bytes:
    if len(audio_data) < 12 or audio_data[:4] != b"RIFF" or audio_data[8:12] != b"WAVE":
        return audio_data

    normalized = bytearray(audio_data)
    struct.pack_into("<I", normalized, 4, max(0, len(normalized) - 8))

    data_offset = find_wav_data_chunk_offset(audio_data)
    if data_offset < 0 or data_offset + 8 > len(normalized):
        return bytes(normalized)

    actual_data_size = max(0, len(normalized) - data_offset - 8)
    struct.pack_into("<I", normalized, data_offset + 4, actual_data_size)
    return bytes(normalized)


def parse_doubao_ws_message(data: bytes) -> DoubaoWSMessage:
    if len(data) < 8:
        raise RuntimeError(f"doubao ws message too short: {len(data)}")
    header_size = 4 * (data[0] & 0x0F)
    if header_size < 4 or len(data) < header_size + 4:
        raise RuntimeError(f"invalid doubao ws header size: {header_size}")

    msg_type = data[1] >> 4
    flag = data[1] & 0x0F
    pos = header_size
    event = 0
    error_code = 0

    if msg_type in (DOUBAO_MSG_TYPE_FULL_SERVER_RESPONSE, DOUBAO_MSG_TYPE_AUDIO_ONLY_SERVER):
        if flag in (DOUBAO_FLAG_POSITIVE_SEQ, DOUBAO_FLAG_NEGATIVE_SEQ):
            pos += 4
    elif msg_type == DOUBAO_MSG_TYPE_ERROR:
        error_code = struct.unpack(">I", data[pos : pos + 4])[0]
        pos += 4

    if flag == DOUBAO_FLAG_WITH_EVENT:
        event = struct.unpack(">i", data[pos : pos + 4])[0]
        pos += 4
        if event not in (
            DOUBAO_EVENT_CONNECTION_STARTED,
            DOUBAO_EVENT_CONNECTION_FAILED,
            DOUBAO_EVENT_CONNECTION_FINISHED,
        ):
            session_id_size = struct.unpack(">I", data[pos : pos + 4])[0]
            pos += 4 + session_id_size
        if event in (
            DOUBAO_EVENT_CONNECTION_STARTED,
            DOUBAO_EVENT_CONNECTION_FAILED,
            DOUBAO_EVENT_CONNECTION_FINISHED,
        ):
            connect_id_size = struct.unpack(">I", data[pos : pos + 4])[0]
            pos += 4 + connect_id_size

    payload_size = struct.unpack(">I", data[pos : pos + 4])[0]
    pos += 4
    payload = data[pos : pos + payload_size]
    return DoubaoWSMessage(
        msg_type=msg_type,
        flag=flag,
        event=event,
        error_code=error_code,
        payload=payload,
    )


class DoubaoWebsocketTTS:
    def __init__(self, settings: DoubaoTTSSettings):
        self.settings = settings
        self._lock = threading.Lock()
        self._queue: "queue.SimpleQueue[str | None]" = queue.SimpleQueue()
        self._worker_started = False
        self._last_error = ""
        self._logger: Callable[[str], None] | None = None

    @classmethod
    def create_from_config(cls, config_path: str, *, base_dir: str = ROOT_DIR) -> "DoubaoWebsocketTTS":
        if os.name != "nt":
            raise RuntimeError("doubao tts playback currently only supports Windows")
        settings = load_doubao_tts_settings(config_path, base_dir=base_dir)
        return cls(settings)

    def _ensure_worker_started(self):
        with self._lock:
            if self._worker_started:
                return
            self._worker_started = True
        threading.Thread(target=self._worker_loop, daemon=True).start()

    def set_logger(self, logger: Callable[[str], None] | None):
        with self._lock:
            self._logger = logger

    def _emit_log(self, line: str):
        logger = self._logger
        if not logger:
            return
        try:
            logger(str(line or ""))
        except Exception:
            pass

    def _worker_loop(self):
        while True:
            payload = self._queue.get()
            if payload is None:
                return
            self._run_speak_blocking(payload)

    async def _synthesize_audio(self, payload: str) -> bytes:
        import websockets

        connect_id = str(uuid.uuid4())
        headers = build_doubao_ws_headers(self.settings, connect_id)
        endpoint_host = urlparse(self.settings.endpoint).netloc or self.settings.endpoint
        preview = summarize_tts_text(payload)
        self._emit_log(
            f"tts synthesize start backend=doubao endpoint={endpoint_host} chars={len(payload)} preview={preview}"
        )
        request_payload = build_doubao_tts_request_payload(self.settings, payload)
        request_frame = build_doubao_full_client_request_frame(
            json.dumps(request_payload, ensure_ascii=False).encode("utf-8")
        )
        websocket = await websockets.connect(
            self.settings.endpoint,
            additional_headers=headers,
            max_size=10 * 1024 * 1024,
            open_timeout=self.settings.connect_timeout_seconds,
        )
        try:
            await websocket.send(request_frame)
            audio_data = bytearray()
            while True:
                response = await websocket.recv()
                if isinstance(response, str):
                    raise RuntimeError(f"unexpected doubao text frame: {response}")
                message = parse_doubao_ws_message(response)
                if message.msg_type == DOUBAO_MSG_TYPE_AUDIO_ONLY_SERVER:
                    audio_data.extend(message.payload)
                    continue
                if message.msg_type == DOUBAO_MSG_TYPE_FULL_SERVER_RESPONSE:
                    if message.event == DOUBAO_EVENT_SESSION_FINISHED:
                        break
                    if message.event in (DOUBAO_EVENT_CONNECTION_FAILED, DOUBAO_EVENT_SESSION_FAILED):
                        detail = message.payload.decode("utf-8", "ignore").strip()
                        raise RuntimeError(detail or f"doubao session failed event={message.event}")
                    continue
                if message.msg_type == DOUBAO_MSG_TYPE_ERROR:
                    detail = message.payload.decode("utf-8", "ignore").strip()
                    raise RuntimeError(f"doubao error code={message.error_code}: {detail}")
            if not audio_data:
                raise RuntimeError("doubao returned empty audio")
            normalized_audio = normalize_wav_size_fields(bytes(audio_data))
            self._emit_log(
                f"tts synthesize success backend=doubao bytes={len(normalized_audio)} chars={len(payload)} preview={preview}"
            )
            return normalized_audio
        finally:
            await websocket.close()

    def _play_wav_bytes(self, audio_data: bytes) -> bool:
        return play_wav_bytes_on_windows(audio_data)

    def _run_speak_blocking(self, payload: str) -> bool:
        preview = summarize_tts_text(payload)
        try:
            audio_data = asyncio.run(self._synthesize_audio(payload))
            self._play_wav_bytes(audio_data)
        except Exception as e:
            self._last_error = str(e)
            self._emit_log(
                f"tts failed backend=doubao error={self._last_error} preview={preview}"
            )
            return False
        self._last_error = ""
        self._emit_log(
            f"tts played backend=doubao bytes={len(audio_data)} chars={len(payload)} preview={preview}"
        )
        return True

    def speak_async(self, text: str) -> bool:
        if os.name != "nt":
            self._emit_log("tts rejected backend=doubao reason=non_windows")
            return False
        payload = normalize_tts_text(text)
        if not is_speakable_english_text(payload):
            self._emit_log(
                f"tts rejected backend=doubao reason=non_speakable preview={summarize_tts_text(payload)}"
            )
            return False
        self._ensure_worker_started()
        self._queue.put(payload)
        self._emit_log(
            f"tts queued backend=doubao chars={len(payload)} preview={summarize_tts_text(payload)}"
        )
        return True


class TencentCloudSDKTTS:
    def __init__(self, settings: TencentCloudTTSSettings):
        self.settings = settings
        self._lock = threading.Lock()
        self._queue: "queue.SimpleQueue[str | None]" = queue.SimpleQueue()
        self._worker_started = False
        self._last_error = ""
        self._logger: Callable[[str], None] | None = None

    @classmethod
    def create_from_config(
        cls,
        config_path: str,
        *,
        base_dir: str = ROOT_DIR,
    ) -> "TencentCloudSDKTTS":
        if os.name != "nt":
            raise RuntimeError("tencent_cloud tts playback currently only supports Windows")
        settings = load_tencent_cloud_tts_settings(config_path, base_dir=base_dir)
        return cls(settings)

    def _ensure_worker_started(self):
        with self._lock:
            if self._worker_started:
                return
            self._worker_started = True
        threading.Thread(target=self._worker_loop, daemon=True).start()

    def set_logger(self, logger: Callable[[str], None] | None):
        with self._lock:
            self._logger = logger

    def _emit_log(self, line: str):
        logger = self._logger
        if not logger:
            return
        try:
            logger(str(line or ""))
        except Exception:
            pass

    def _worker_loop(self):
        while True:
            payload = self._queue.get()
            if payload is None:
                return
            self._run_speak_blocking(payload)

    def _synthesize_audio(self, payload: str) -> tuple[bytes, str, str]:
        credential_module = importlib.import_module("tencentcloud.common.credential")
        client_profile_module = importlib.import_module(
            "tencentcloud.common.profile.client_profile"
        )
        http_profile_module = importlib.import_module("tencentcloud.common.profile.http_profile")
        tts_client_module = importlib.import_module("tencentcloud.tts.v20190823.tts_client")
        tts_models_module = importlib.import_module("tencentcloud.tts.v20190823.models")

        preview = summarize_tts_text(payload)
        self._emit_log(
            f"tts synthesize start backend=tencent_cloud endpoint={self.settings.endpoint} chars={len(payload)} preview={preview}"
        )
        credential_obj = credential_module.Credential(
            self.settings.secret_id,
            self.settings.secret_key,
        )
        http_profile = http_profile_module.HttpProfile(
            endpoint=self.settings.endpoint,
            reqTimeout=max(1, int(math.ceil(self.settings.request_timeout_seconds))),
        )
        client_profile = client_profile_module.ClientProfile(httpProfile=http_profile)
        client = tts_client_module.TtsClient(
            credential_obj,
            self.settings.region,
            client_profile,
        )

        session_id = str(uuid.uuid4())
        request_payload = build_tencent_cloud_tts_request_payload(
            self.settings,
            payload,
            session_id,
        )
        request_model = tts_models_module.TextToVoiceRequest()
        request_model.Text = request_payload["Text"]
        request_model.SessionId = request_payload["SessionId"]
        request_model.Volume = request_payload["Volume"]
        request_model.Speed = request_payload["Speed"]
        request_model.ProjectId = request_payload["ProjectId"]
        request_model.ModelType = request_payload["ModelType"]
        request_model.VoiceType = request_payload["VoiceType"]
        request_model.PrimaryLanguage = request_payload["PrimaryLanguage"]
        request_model.SampleRate = request_payload["SampleRate"]
        request_model.Codec = request_payload["Codec"]
        request_model.EnableSubtitle = request_payload["EnableSubtitle"]
        request_model.SegmentRate = request_payload["SegmentRate"]
        if "EmotionCategory" in request_payload:
            request_model.EmotionCategory = request_payload["EmotionCategory"]
            request_model.EmotionIntensity = request_payload["EmotionIntensity"]

        response = client.TextToVoice(request_model)
        request_id = str(getattr(response, "RequestId", "") or "").strip()
        response_session_id = str(getattr(response, "SessionId", "") or session_id).strip()
        encoded_audio = str(getattr(response, "Audio", "") or "").strip()
        if not encoded_audio:
            raise RuntimeError(
                f"tencent_cloud returned empty audio request_id={request_id or '-'}"
            )
        try:
            audio_data = base64.b64decode(encoded_audio)
        except Exception as e:
            raise RuntimeError(
                f"tencent_cloud returned invalid base64 audio request_id={request_id or '-'}"
            ) from e
        if not audio_data:
            raise RuntimeError(
                f"tencent_cloud decoded empty audio request_id={request_id or '-'}"
            )
        self._emit_log(
            f"tts synthesize success backend=tencent_cloud request_id={request_id or '-'} "
            f"session_id={response_session_id or '-'} bytes={len(audio_data)} chars={len(payload)} preview={preview}"
        )
        return audio_data, request_id, response_session_id

    def _play_wav_bytes(self, audio_data: bytes) -> bool:
        return play_wav_bytes_on_windows(audio_data)

    def _run_speak_blocking(self, payload: str) -> bool:
        preview = summarize_tts_text(payload)
        request_id = ""
        response_session_id = ""
        try:
            audio_data, request_id, response_session_id = self._synthesize_audio(payload)
            self._play_wav_bytes(audio_data)
        except Exception as e:
            self._last_error = format_tencent_cloud_sdk_exception(e)
            self._emit_log(
                f"tts failed backend=tencent_cloud error={self._last_error} preview={preview}"
            )
            return False
        self._last_error = ""
        self._emit_log(
            f"tts played backend=tencent_cloud request_id={request_id or '-'} "
            f"session_id={response_session_id or '-'} bytes={len(audio_data)} chars={len(payload)} preview={preview}"
        )
        return True

    def speak_async(self, text: str) -> bool:
        if os.name != "nt":
            self._emit_log("tts rejected backend=tencent_cloud reason=non_windows")
            return False
        payload = normalize_tts_text(text)
        if not is_speakable_english_text(payload):
            self._emit_log(
                f"tts rejected backend=tencent_cloud reason=non_speakable preview={summarize_tts_text(payload)}"
            )
            return False
        self._ensure_worker_started()
        self._queue.put(payload)
        self._emit_log(
            f"tts queued backend=tencent_cloud chars={len(payload)} preview={summarize_tts_text(payload)}"
        )
        return True


def create_tts_player(
    tts_cfg: dict[str, Any],
    *,
    config_dir: str = ROOT_DIR,
) -> tuple[Any | None, str]:
    provider = normalize_tts_provider(tts_cfg.get("provider", DEFAULT_TTS_PROVIDER))
    if provider == "windows_system":
        player = WindowsSystemTTS.create_default()
        if player:
            return (
                player,
                "tts configured backend=windows_system voice_probe=lazy preferred=Microsoft Zira Desktop,Microsoft David Desktop",
            )
        return None, "tts unavailable: no english system voice detected"

    if provider == "doubao":
        config_path = str(tts_cfg.get("config_path") or DOUBAO_TTS_DEFAULT_CONFIG_PATH).strip()
        resolved_config_path = resolve_config_file_path(config_path, base_dir=config_dir)
        settings = load_doubao_tts_settings(config_path, base_dir=config_dir)
        runtime_fields = build_doubao_tts_runtime_fields(settings, resolved_config_path)
        dependency_error = probe_doubao_websocket_runtime()
        if dependency_error:
            return None, f"tts unavailable {runtime_fields} reason={dependency_error}"
        player = DoubaoWebsocketTTS(settings)
        return (
            player,
            f"tts configured {runtime_fields}",
        )

    config_path = str(tts_cfg.get("config_path") or TENCENT_CLOUD_TTS_DEFAULT_CONFIG_PATH).strip()
    resolved_config_path = resolve_config_file_path(config_path, base_dir=config_dir)
    settings = load_tencent_cloud_tts_settings(config_path, base_dir=config_dir)
    runtime_fields = build_tencent_cloud_tts_runtime_fields(settings, resolved_config_path)
    dependency_error = probe_tencent_cloud_tts_runtime()
    if dependency_error:
        return None, f"tts unavailable {runtime_fields} reason={dependency_error}"
    player = TencentCloudSDKTTS(settings)
    return (
        player,
        f"tts configured {runtime_fields}",
    )


def check_tts_dependency_packaging(
    tts_cfg: dict[str, Any],
) -> tuple[bool, str]:
    provider = normalize_tts_provider(tts_cfg.get("provider", DEFAULT_TTS_PROVIDER))
    if provider == "windows_system":
        return True, "tts dependency check passed backend=windows_system"
    if provider == "doubao":
        dependency_error = probe_doubao_websocket_runtime()
        if dependency_error:
            return False, f"tts dependency check failed backend=doubao reason={dependency_error}"
        return True, "tts dependency check passed backend=doubao module=websockets"
    dependency_error = probe_tencent_cloud_tts_runtime()
    if dependency_error:
        return False, f"tts dependency check failed backend=tencent_cloud reason={dependency_error}"
    return True, "tts dependency check passed backend=tencent_cloud module=tencentcloud"


def cleanup_dedupe_cache(cache: Dict[str, float], now_ts: float):
    expired = [key for key, ts in cache.items() if now_ts - ts > DEDUPE_CACHE_TTL_SECONDS]
    for key in expired:
        cache.pop(key, None)

    overflow = len(cache) - DEDUPE_CACHE_MAX_KEYS
    if overflow > 0:
        oldest = sorted(cache.items(), key=lambda item: item[1])[:overflow]
        for key, _ in oldest:
            cache.pop(key, None)


def build_worker_status_text(state: str, detail: str, target_count: int) -> str:
    clean_state = str(state or "idle").strip() or "idle"
    if clean_state == "running":
        return ""
    if clean_state == "starting":
        return "启动中"
    if clean_state == "waiting_wechat":
        return "等待微信"
    if clean_state == "connecting":
        return "连接微信"
    if clean_state == "window_lost":
        return "微信窗口丢失"
    if clean_state == "reconnecting":
        return "重新连接中"
    if clean_state == "worker_backoff":
        return "监听中断，稍后重试"
    if clean_state == "stopped":
        return "已停止"
    clean_detail = str(detail or "").strip()
    return clean_detail


def compute_worker_restart_delay(attempt: int) -> float:
    safe_attempt = max(1, int(attempt))
    delay = WORKER_RESTART_INITIAL_BACKOFF_SECONDS * (2 ** (safe_attempt - 1))
    return min(delay, WORKER_RESTART_MAX_BACKOFF_SECONDS)


def truncate_target_label(name: str, max_chars: int = TARGET_LABEL_MAX_CHARS) -> str:
    text = str(name or "").strip()
    if max_chars <= 0:
        return ""
    if len(text) <= max_chars:
        return text
    return text[:max_chars] + "..."


def build_sidebar_window_title(chat_name: str) -> str:
    name = str(chat_name or "").strip()
    if name:
        return name
    return "未选择会话"


def pick_ui_font_family(root: tk.Tk) -> str:
    try:
        available = {name.lower(): name for name in tkfont.families(root)}
    except Exception:
        available = {}

    for font_name in PREFERRED_UI_FONTS:
        chosen = available.get(font_name.lower())
        if chosen:
            return chosen

    return str(tkfont.nametofont("TkDefaultFont").cget("family"))


class TranslatorBase:
    def translate(self, text: str) -> str:
        raise NotImplementedError


class PassthroughTranslator(TranslatorBase):
    def translate(self, text: str) -> str:
        return text


class DeepLXTranslator(TranslatorBase):
    def __init__(
        self,
        url: str,
        source_lang: str = "auto",
        target_lang: str = "EN",
        timeout_seconds: float = 8.0,
        retry_attempts: int = DEEPLX_MAX_RETRIES,
        retry_backoff_seconds: float = DEEPLX_RETRY_BACKOFF_SECONDS,
    ):
        self.url = url.rstrip("/")
        self.source_lang = source_lang
        self.target_lang = target_lang
        self.timeout_seconds = timeout_seconds
        self.retry_attempts = max(0, int(retry_attempts))
        self.retry_backoff_seconds = max(0.0, float(retry_backoff_seconds))

    def translate(self, text: str) -> str:
        payload = json.dumps(
            {
                "text": text,
                "source_lang": self.source_lang,
                "target_lang": self.target_lang,
            }
        ).encode("utf-8")
        headers = {
            "Content-Type": "application/json; charset=utf-8",
            "Accept": "application/json,text/plain,*/*",
            # 某些 DeepLX 网关会按 UA/请求特征做风控，Python 默认 UA 可能被 403 拦截。
            "User-Agent": (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
                "AppleWebKit/537.36 (KHTML, like Gecko) "
                "Chrome/124.0.0.0 Safari/537.36"
            ),
        }
        parsed = urlparse(self.url)
        if parsed.netloc.lower().endswith("deeplx.org"):
            headers["Origin"] = "https://api.deeplx.org"
            headers["Referer"] = "https://api.deeplx.org/"
        req = request.Request(
            self.url,
            data=payload,
            headers=headers,
            method="POST",
        )
        raw = ""
        total_attempts = self.retry_attempts + 1
        for attempt in range(1, total_attempts + 1):
            try:
                with request.urlopen(req, timeout=self.timeout_seconds) as resp:
                    raw = resp.read().decode("utf-8", errors="ignore")
                break
            except error.HTTPError as e:
                body = ""
                try:
                    body = e.read().decode("utf-8", errors="ignore")
                except Exception:
                    pass
                raise RuntimeError(
                    f"DeepLX request failed: HTTP {e.code}, body={body[:120]}"
                ) from e
            except error.URLError as e:
                if attempt >= total_attempts:
                    raise RuntimeError(
                        f"DeepLX request failed after {total_attempts} attempts: {e}"
                    ) from e
                if self.retry_backoff_seconds > 0:
                    time.sleep(self.retry_backoff_seconds * attempt)

        try:
            body = json.loads(raw)
        except json.JSONDecodeError as e:
            raise RuntimeError(f"DeepLX response invalid JSON: {raw[:200]}") from e

        if isinstance(body, dict):
            for key in ("data", "translation", "text"):
                value = body.get(key)
                if isinstance(value, str) and value.strip():
                    return value.strip()

            translations = body.get("translations")
            if isinstance(translations, list) and translations:
                first = translations[0]
                if isinstance(first, dict):
                    text_value = first.get("text")
                    if isinstance(text_value, str) and text_value.strip():
                        return text_value.strip()

        raise RuntimeError(f"DeepLX response unsupported: {raw[:200]}")


@dataclass
class SidebarMessage:
    chat_name: str
    sender_name: str
    text_en: str
    created_at: str
    is_self: bool
    text_display: str = ""
    text_cn: str = ""
    message_id: str = ""
    pending_translation: bool = False


def append_message_with_limit(cache: list[SidebarMessage], msg: SidebarMessage, limit: int):
    cache.append(msg)
    if limit <= 0:
        cache.clear()
        return
    overflow = len(cache) - limit
    if overflow > 0:
        del cache[:overflow]


def replace_message_in_cache(cache: list[SidebarMessage], msg: SidebarMessage) -> bool:
    message_id = str(msg.message_id or "").strip()
    if not message_id:
        return False
    for idx, existing in enumerate(cache):
        if str(existing.message_id or "").strip() == message_id:
            cache[idx] = msg
            return True
    return False


def append_log_file(path: str, line: str):
    if not path:
        return
    try:
        with _LOG_WRITE_LOCK:
            rotate_log_file_if_needed(path)
            with open(path, "a", encoding="utf-8") as f:
                f.write(line.rstrip() + "\n")
    except Exception:
        pass


def resolve_log_file_path(path: Any) -> str:
    raw = str(path or "").strip()
    if not raw:
        return ""
    resolved = raw if os.path.isabs(raw) else os.path.join(ROOT_DIR, raw)
    normalized = os.path.normpath(resolved)
    parent = os.path.dirname(normalized)
    if parent:
        try:
            os.makedirs(parent, exist_ok=True)
        except Exception:
            pass
    return normalized


def resolve_worker_executable_path(runtime_root: str = ROOT_DIR) -> str:
    return os.path.join(runtime_root, WORKER_EXE_NAME)


def build_worker_command(
    targets: list[str],
    interval: float,
    debug: bool,
    focus_refresh: bool,
    load_retry_seconds: float,
    *,
    frozen: bool | None = None,
    python_executable: str | None = None,
    source_root: str = SOURCE_ROOT,
    runtime_root: str = ROOT_DIR,
) -> list[str]:
    use_frozen = is_frozen_app() if frozen is None else bool(frozen)
    if use_frozen:
        cmd = [
            resolve_worker_executable_path(runtime_root),
            "--targets-json",
            json.dumps(targets, ensure_ascii=False),
            "--interval",
            str(interval),
            "--load-retry-seconds",
            str(load_retry_seconds),
        ]
    else:
        worker_script = os.path.join(source_root, "listener_app", "group_listener_worker.py")
        cmd = [
            python_executable or sys.executable,
            "-X",
            "utf8",
            "-u",
            worker_script,
            "--targets-json",
            json.dumps(targets, ensure_ascii=False),
            "--interval",
            str(interval),
            "--load-retry-seconds",
            str(load_retry_seconds),
        ]
    if debug:
        cmd.append("--debug")
    if focus_refresh:
        cmd.append("--focus-refresh")
    return cmd


def rotate_log_file_if_needed(path: str):
    try:
        if not os.path.exists(path):
            return
        if os.path.getsize(path) < LOG_ROTATE_MAX_BYTES:
            return
    except Exception:
        return

    try:
        oldest = f"{path}.{LOG_ROTATE_KEEP_FILES}"
        if os.path.exists(oldest):
            os.remove(oldest)
    except Exception:
        pass

    for idx in range(LOG_ROTATE_KEEP_FILES - 1, 0, -1):
        src = f"{path}.{idx}"
        dst = f"{path}.{idx + 1}"
        try:
            if os.path.exists(src):
                os.replace(src, dst)
        except Exception:
            pass

    try:
        os.replace(path, f"{path}.1")
    except Exception:
        pass


def _target_lock_path(target: str) -> str:
    os.makedirs(RUNTIME_LOCK_DIR, exist_ok=True)
    digest = hashlib.sha1(target.strip().lower().encode("utf-8")).hexdigest()[:12]
    return os.path.join(RUNTIME_LOCK_DIR, f"target_{digest}.lock")


def _read_lock_payload(lock_path: str) -> dict[str, Any]:
    try:
        with open(lock_path, "r", encoding="utf-8") as f:
            data = json.load(f)
        if isinstance(data, dict):
            return data
    except Exception:
        pass
    return {}


def _is_pid_alive(pid: int) -> bool:
    if pid <= 0:
        return False
    if os.name == "nt":
        try:
            import ctypes

            PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
            SYNCHRONIZE = 0x00100000
            STILL_ACTIVE = 259

            kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
            kernel32.OpenProcess.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32]
            kernel32.OpenProcess.restype = ctypes.c_void_p
            kernel32.GetExitCodeProcess.argtypes = [
                ctypes.c_void_p,
                ctypes.POINTER(ctypes.c_uint32),
            ]
            kernel32.GetExitCodeProcess.restype = ctypes.c_int
            kernel32.CloseHandle.argtypes = [ctypes.c_void_p]
            kernel32.CloseHandle.restype = ctypes.c_int

            handle = kernel32.OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
                False,
                int(pid),
            )
            if not handle:
                return False
            try:
                exit_code = ctypes.c_uint32()
                ok = kernel32.GetExitCodeProcess(handle, ctypes.byref(exit_code))
                if not ok:
                    return False
                return int(exit_code.value) == STILL_ACTIVE
            finally:
                kernel32.CloseHandle(handle)
        except Exception:
            return False
    try:
        os.kill(pid, 0)
    except PermissionError:
        return True
    except OSError:
        return False
    return True


def _get_pid_start_token(pid: int) -> str:
    if pid <= 0 or os.name != "nt":
        return ""
    try:
        import ctypes

        PROCESS_QUERY_LIMITED_INFORMATION = 0x1000

        class FILETIME(ctypes.Structure):
            _fields_ = [("dwLowDateTime", ctypes.c_uint32), ("dwHighDateTime", ctypes.c_uint32)]

        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.OpenProcess.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32]
        kernel32.OpenProcess.restype = ctypes.c_void_p
        kernel32.GetProcessTimes.argtypes = [
            ctypes.c_void_p,
            ctypes.POINTER(FILETIME),
            ctypes.POINTER(FILETIME),
            ctypes.POINTER(FILETIME),
            ctypes.POINTER(FILETIME),
        ]
        kernel32.GetProcessTimes.restype = ctypes.c_int
        kernel32.CloseHandle.argtypes = [ctypes.c_void_p]
        kernel32.CloseHandle.restype = ctypes.c_int

        handle = kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, int(pid))
        if not handle:
            return ""
        creation = FILETIME()
        exit_time = FILETIME()
        kernel_time = FILETIME()
        user_time = FILETIME()
        ok = kernel32.GetProcessTimes(
            handle,
            ctypes.byref(creation),
            ctypes.byref(exit_time),
            ctypes.byref(kernel_time),
            ctypes.byref(user_time),
        )
        kernel32.CloseHandle(handle)
        if not ok:
            return ""

        ticks = (int(creation.dwHighDateTime) << 32) | int(creation.dwLowDateTime)
        return str(ticks)
    except Exception:
        return ""


def _is_lock_owner_alive(lock_payload: dict[str, Any]) -> bool:
    pid = as_int(lock_payload.get("pid"), 0)
    if not _is_pid_alive(pid):
        return False

    expected_token = str(lock_payload.get("start_token", "")).strip()
    if not expected_token:
        return True

    current_token = _get_pid_start_token(pid)
    if not current_token:
        # 无法读取启动时间时退化为 PID 存活判断，避免误删活动锁。
        return True
    return current_token == expected_token


def is_target_already_running(target: str) -> bool:
    lock_path = _target_lock_path(target)
    if not os.path.exists(lock_path):
        return False
    lock_payload = _read_lock_payload(lock_path)
    if _is_lock_owner_alive(lock_payload):
        return True
    try:
        os.remove(lock_path)
    except Exception:
        pass
    return False


def cleanup_stale_target_locks() -> int:
    if not os.path.isdir(RUNTIME_LOCK_DIR):
        return 0

    removed = 0
    try:
        entries = os.listdir(RUNTIME_LOCK_DIR)
    except Exception:
        return 0

    for name in entries:
        if not name.startswith("target_") or not name.endswith(".lock"):
            continue
        lock_path = os.path.join(RUNTIME_LOCK_DIR, name)
        if not os.path.isfile(lock_path):
            continue

        lock_payload = _read_lock_payload(lock_path)
        if _is_lock_owner_alive(lock_payload):
            continue

        try:
            os.remove(lock_path)
            removed += 1
        except Exception:
            pass
    return removed


def acquire_target_lock(target: str) -> tuple[bool, str]:
    lock_path = _target_lock_path(target)
    if os.path.exists(lock_path):
        existing = _read_lock_payload(lock_path)
        if _is_lock_owner_alive(existing):
            pid = as_int(existing.get("pid"), 0)
            return False, f"target already running pid={pid}"
        try:
            os.remove(lock_path)
        except Exception:
            pass

    start_token = _get_pid_start_token(os.getpid())
    payload = {
        "pid": os.getpid(),
        "target": target,
        "created_at": datetime.now().isoformat(timespec="seconds"),
    }
    if start_token:
        payload["start_token"] = start_token
    try:
        with open(lock_path, "x", encoding="utf-8") as f:
            json.dump(payload, f, ensure_ascii=False)
    except FileExistsError:
        existing = _read_lock_payload(lock_path)
        if _is_lock_owner_alive(existing):
            pid = as_int(existing.get("pid"), 0)
            return False, f"target already running pid={pid}"
        try:
            with open(lock_path, "w", encoding="utf-8") as f:
                json.dump(payload, f, ensure_ascii=False)
        except Exception as e:
            return False, f"target lock create failed: {e}"
    except Exception as e:
        return False, f"target lock create failed: {e}"
    return True, lock_path


def _release_lock_paths(lock_paths: list[str]):
    for path in lock_paths:
        if not path:
            continue
        try:
            lock_payload = _read_lock_payload(path)
            pid = as_int(lock_payload.get("pid"), 0)
            expected_token = str(lock_payload.get("start_token", "")).strip()
            current_token = _get_pid_start_token(os.getpid())
            same_process = pid == os.getpid()
            if expected_token and current_token and expected_token != current_token:
                same_process = False
            if same_process and os.path.exists(path):
                os.remove(path)
        except Exception:
            pass


def release_target_lock_path(lock_path: str):
    global _TARGET_LOCK_PATHS
    path = str(lock_path or "").strip()
    if not path:
        return
    _TARGET_LOCK_PATHS = [item for item in _TARGET_LOCK_PATHS if item != path]
    _release_lock_paths([path])


def release_target_lock():
    global _TARGET_LOCK_PATHS
    lock_paths = list(_TARGET_LOCK_PATHS)
    _TARGET_LOCK_PATHS = []
    _release_lock_paths(lock_paths)


def normalize_translate_provider(value: Any) -> str:
    provider = str(value or "deeplx").strip().lower() or "deeplx"
    if provider not in SUPPORTED_TRANSLATE_PROVIDERS:
        supported = ", ".join(SUPPORTED_TRANSLATE_PROVIDERS)
        raise RuntimeError(f"translate.provider must be one of: {supported}")
    return provider


def validate_translate_config(enabled: bool, provider: str, deeplx_url: str):
    if not enabled:
        return
    if provider == "deeplx" and not str(deeplx_url or "").strip():
        raise RuntimeError(
            "translate.enabled=true and provider=deeplx require translate.deeplx_url or DEEPLX_URL"
        )


def maybe_show_frozen_error_dialog(message: str, title: str = "wechat_sidebar 启动失败"):
    if not is_frozen_app() or os.name != "nt":
        return
    try:
        import ctypes

        ctypes.windll.user32.MessageBoxW(None, message, title, 0x10)
    except Exception:
        pass


def exit_startup_error(message: str, exit_code: int = 2):
    print(f"[sidebar] {message}", file=sys.stderr)
    maybe_show_frozen_error_dialog(message)
    raise SystemExit(exit_code)


def build_translator_runtime_text(enabled: bool, provider: str) -> str:
    if not enabled:
        return "translator=passthrough reason=translate.disabled"
    if provider == "passthrough":
        return "translator=passthrough reason=provider=passthrough"
    return (
        f"translator=deeplx attempts={DEEPLX_MAX_RETRIES + 1} "
        f"retry_backoff={DEEPLX_RETRY_BACKOFF_SECONDS:.1f}s"
    )


def create_translator(
    enabled: bool,
    provider: str,
    deeplx_url: str,
    source_lang: str,
    target_lang: str,
    timeout_seconds: float,
) -> TranslatorBase:
    if not enabled:
        return PassthroughTranslator()
    if provider == "passthrough":
        return PassthroughTranslator()
    if provider != "deeplx":
        raise RuntimeError(f"unsupported translator provider: {provider}")
    if not deeplx_url:
        raise RuntimeError(
            "translate.enabled=true and provider=deeplx require translate.deeplx_url or DEEPLX_URL"
        )
    return DeepLXTranslator(
        url=deeplx_url,
        source_lang=source_lang,
        target_lang=target_lang,
        timeout_seconds=timeout_seconds,
    )


def build_translate_fallback(cn_text: str, err: Exception, behavior: str) -> str:
    reason = str(err).replace("\n", " ").strip()
    if len(reason) > 200:
        reason = reason[:200]
    if behavior == "show_cn":
        return cn_text
    if behavior == "show_reason":
        return f"translate_failed: {reason}"
    return f"{cn_text} (translate_failed: {reason})"


class SidebarUI:
    def __init__(
        self,
        width: int,
        side: str,
        targets: list[str],
        message_limit: int,
        tts_player: WindowsSystemTTS | None = None,
        tts_auto_read_active_chat: bool = True,
    ):
        self.root = tk.Tk()
        self.root.title(build_sidebar_window_title(""))
        self.ui_font_family = pick_ui_font_family(self.root)
        self.root.option_add("*Font", (self.ui_font_family, DEFAULT_META_FONT_SIZE))
        self.topmost_var = tk.BooleanVar(value=False)
        self.show_original_var = tk.BooleanVar(value=False)
        self.tts_auto_read_var = tk.BooleanVar(value=bool(tts_auto_read_active_chat))
        self.root.attributes("-topmost", self.topmost_var.get())
        self.status_var = tk.StringVar(value="starting...")
        self.target_panel_visible = False
        self.target_panel_toggle_text = tk.StringVar(value="菜单")
        self.message_limit = max(1, int(message_limit))
        self.tts_player = tts_player
        self.tts_auto_read_active_chat = bool(self.tts_auto_read_var.get())
        self.runtime_logger: Callable[[str], None] | None = None
        self.allowed_chat_names = {str(item or "").strip() for item in targets if str(item or "").strip()}
        self.chat_order: list[str] = []
        self.chat_messages: dict[str, list[SidebarMessage]] = {}
        self.unread_counts: dict[str, int] = {}
        self.active_chat = ""
        self._on_add_target_request: Callable[[str], None] | None = None
        self._on_remove_target_request: Callable[[str], None] | None = None
        self._context_menu_target = ""
        self._tts_action_tags: dict[str, str] = {}
        self._tts_action_index = 0
        self._tts_body_click_press: dict[str, Any] | None = None
        self._tts_body_click_pending_after_id = ""
        self._tts_body_click_pending_tag = ""
        self._last_chat_switch_shortcut_at = 0.0

        screen_w = self.root.winfo_screenwidth()
        screen_h = self.root.winfo_screenheight()
        height = min(DEFAULT_SIDEBAR_HEIGHT, max(320, screen_h - 80))
        if side == "right":
            x = screen_w - width - 16
        else:
            x = 16
        max_x = max(0, screen_w - width - 8)
        x = min(max(0, x), max_x)
        y = 24
        max_y = max(24, screen_h - height - 24)
        if y > max_y:
            y = max_y
        self.root.geometry(f"{width}x{height}+{x}+{y}")

        controls = ttk.Frame(self.root, padding=(8, 6, 8, 4))
        controls.pack(fill=tk.X)
        ttk.Button(
            controls,
            textvariable=self.target_panel_toggle_text,
            command=self.toggle_target_panel,
            width=6,
        ).pack(side=tk.LEFT, padx=(0, 6))
        self.add_target_button = ttk.Button(
            controls,
            text="添加群",
            command=self._on_add_target_clicked,
            width=8,
            state=tk.DISABLED,
        )
        self.add_target_button.pack(side=tk.LEFT, padx=(0, 6))
        ttk.Checkbutton(
            controls,
            text="原文",
            variable=self.show_original_var,
            command=self._render_active_chat,
        ).pack(side=tk.LEFT, padx=(0, 6))
        ttk.Checkbutton(
            controls,
            text="朗读",
            variable=self.tts_auto_read_var,
            command=self.toggle_auto_read,
        ).pack(side=tk.LEFT, padx=(0, 6))
        ttk.Checkbutton(
            controls,
            text="置顶",
            variable=self.topmost_var,
            command=self.toggle_topmost,
        ).pack(side=tk.LEFT)
        ttk.Label(controls, textvariable=self.status_var).pack(
            side=tk.LEFT, fill=tk.X, expand=True
        )

        content = ttk.Frame(self.root)
        content.pack(fill=tk.BOTH, expand=True, padx=8, pady=(0, 8))

        self.left_panel = ttk.Frame(content, width=DEFAULT_TARGET_PANEL_WIDTH)
        self.left_panel.pack_propagate(False)
        self.target_list = tk.Listbox(
            self.left_panel,
            exportselection=False,
            activestyle="none",
            font=(self.ui_font_family, DEFAULT_META_FONT_SIZE),
        )
        self.target_list.pack(fill=tk.BOTH, expand=True)
        self.target_list.bind("<<ListboxSelect>>", self._on_target_selected)
        self.target_list.bind("<Button-3>", self._on_target_context_menu)
        self.target_context_menu = tk.Menu(self.root, tearoff=0)
        self.target_context_menu.add_command(
            label="删除监听目标",
            command=self._on_remove_target_menu,
        )

        self.text = scrolledtext.ScrolledText(
            content,
            wrap=tk.WORD,
            font=(self.ui_font_family, DEFAULT_META_FONT_SIZE),
            state=tk.DISABLED,
        )
        self.text.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        message_font_size = DEFAULT_META_FONT_SIZE + MESSAGE_FONT_EXTRA_PX
        self.text.tag_configure(
            "msg_left",
            justify=tk.LEFT,
            lmargin1=8,
            lmargin2=8,
            rmargin=40,
            font=(self.ui_font_family, message_font_size),
        )
        self.text.tag_configure(
            "msg_right",
            justify=tk.RIGHT,
            lmargin1=40,
            lmargin2=40,
            rmargin=8,
            font=(self.ui_font_family, message_font_size),
        )
        self.text.tag_configure(
            "msg_left_pending",
            justify=tk.LEFT,
            lmargin1=8,
            lmargin2=8,
            rmargin=40,
            font=(self.ui_font_family, message_font_size),
            foreground=META_TEXT_COLOR,
        )
        self.text.tag_configure(
            "msg_right_pending",
            justify=tk.RIGHT,
            lmargin1=40,
            lmargin2=40,
            rmargin=8,
            font=(self.ui_font_family, message_font_size),
            foreground=META_TEXT_COLOR,
        )
        self.text.tag_configure("meta_left", justify=tk.LEFT, foreground=META_TEXT_COLOR)
        self.text.tag_configure("meta_right", justify=tk.RIGHT, foreground=META_TEXT_COLOR)

        for target in targets:
            self._ensure_chat(target)
        if self.chat_order:
            self.switch_chat(self.chat_order[0])
        self._set_target_panel_visible(True)
        self.root.bind("<Control-b>", self.on_toggle_target_panel_shortcut)
        self.root.bind("<Control-B>", self.on_toggle_target_panel_shortcut)
        self.root.bind("<Control-Up>", self.on_switch_prev_chat_shortcut)
        self.root.bind("<Control-Down>", self.on_switch_next_chat_shortcut)

    def set_status(self, text: str):
        self.status_var.set(text)

    def set_target_editor_handlers(
        self,
        on_add_target: Callable[[str], None] | None,
        on_remove_target: Callable[[str], None] | None,
    ):
        self._on_add_target_request = on_add_target
        self._on_remove_target_request = on_remove_target
        self.add_target_button.configure(state=tk.NORMAL if on_add_target else tk.DISABLED)

    def toggle_topmost(self):
        self.root.attributes("-topmost", self.topmost_var.get())

    def toggle_auto_read(self):
        self.tts_auto_read_active_chat = self._is_auto_read_enabled()

    def toggle_target_panel(self):
        self._set_target_panel_visible(not self.target_panel_visible)

    def on_toggle_target_panel_shortcut(self, _event=None):
        self.toggle_target_panel()
        return "break"

    def on_switch_prev_chat_shortcut(self, _event=None):
        self._handle_chat_switch_shortcut(-1)
        return "break"

    def on_switch_next_chat_shortcut(self, _event=None):
        self._handle_chat_switch_shortcut(1)
        return "break"

    def _handle_chat_switch_shortcut(self, delta: int):
        if not self.chat_order:
            return
        now_ts = time.time()
        if (
            self._last_chat_switch_shortcut_at
            and now_ts - self._last_chat_switch_shortcut_at < CHAT_SWITCH_SHORTCUT_DEBOUNCE_SECONDS
        ):
            return
        self._last_chat_switch_shortcut_at = now_ts
        if self.active_chat not in self.chat_order:
            self.switch_chat(self.chat_order[0])
            return
        current_index = self.chat_order.index(self.active_chat)
        next_index = (current_index + int(delta)) % len(self.chat_order)
        self.switch_chat(self.chat_order[next_index])

    def _set_target_panel_visible(self, visible: bool):
        if visible:
            self.left_panel.pack(side=tk.LEFT, fill=tk.Y, padx=(0, 8), before=self.text)
            self.target_panel_toggle_text.set("收起")
            self.target_panel_visible = True
            return
        self.left_panel.pack_forget()
        self.target_panel_toggle_text.set("菜单")
        self.target_panel_visible = False

    def _update_window_title(self):
        self.root.title(build_sidebar_window_title(self.active_chat))

    def _ensure_chat(self, chat_name: str):
        name = str(chat_name or "").strip()
        if not name:
            return False
        allowed_chat_names = getattr(self, "allowed_chat_names", set())
        if allowed_chat_names and name not in allowed_chat_names:
            return False
        if name not in self.chat_messages:
            self.chat_messages[name] = []
            self.unread_counts[name] = 0
            self.chat_order.append(name)
            self._refresh_target_list()
        return True

    def add_target(self, chat_name: str) -> bool:
        name = str(chat_name or "").strip()
        if not name:
            return False
        self.allowed_chat_names.add(name)
        created = self._ensure_chat(name)
        if not self.active_chat:
            self.switch_chat(name)
        else:
            self._refresh_target_list()
        return created

    def remove_target(self, chat_name: str) -> str:
        name = str(chat_name or "").strip()
        if not name:
            return self.active_chat
        self.allowed_chat_names.discard(name)
        self.chat_messages.pop(name, None)
        self.unread_counts.pop(name, None)
        if name in self.chat_order:
            self.chat_order.remove(name)
        if self.active_chat == name:
            self.active_chat = self.chat_order[0] if self.chat_order else ""
        self._update_window_title()
        self._refresh_target_list()
        self._render_active_chat()
        return self.active_chat

    def _format_chat_label(self, chat_name: str) -> str:
        display_name = truncate_target_label(chat_name)
        unread = self.unread_counts.get(chat_name, 0)
        if unread > 0:
            return f"{display_name} ({unread})"
        return display_name

    def _refresh_target_list(self):
        self.target_list.delete(0, tk.END)
        for chat_name in self.chat_order:
            self.target_list.insert(tk.END, self._format_chat_label(chat_name))
        if self.active_chat in self.chat_order:
            idx = self.chat_order.index(self.active_chat)
            self.target_list.selection_clear(0, tk.END)
            self.target_list.selection_set(idx)
            self.target_list.activate(idx)

    def _on_target_selected(self, _event=None):
        selection = self.target_list.curselection()
        if not selection:
            return
        idx = int(selection[0])
        if idx < 0 or idx >= len(self.chat_order):
            return
        self.switch_chat(self.chat_order[idx])

    def _on_target_context_menu(self, event):
        if not self.chat_order or not self._on_remove_target_request:
            return "break"
        idx = self.target_list.nearest(event.y)
        if idx < 0 or idx >= len(self.chat_order):
            return "break"
        bbox = self.target_list.bbox(idx)
        if not bbox:
            return "break"
        _, item_y, _, item_height = bbox
        if event.y < item_y or event.y > item_y + item_height:
            return "break"
        self.target_list.selection_clear(0, tk.END)
        self.target_list.selection_set(idx)
        self.target_list.activate(idx)
        self._context_menu_target = self.chat_order[idx]
        try:
            self.target_context_menu.tk_popup(event.x_root, event.y_root)
        finally:
            self.target_context_menu.grab_release()
        return "break"

    def _on_add_target_clicked(self):
        handler = self._on_add_target_request
        if not handler:
            return
        raw_name = simpledialog.askstring(
            "添加监听目标",
            "输入微信左侧会话名（必须完全一致）",
            parent=self.root,
        )
        if raw_name is None:
            return
        target_name = str(raw_name or "").strip()
        if not target_name:
            messagebox.showerror("添加失败", "会话名不能为空", parent=self.root)
            return
        try:
            handler(target_name)
        except Exception as e:
            messagebox.showerror("添加失败", str(e), parent=self.root)

    def _on_remove_target_menu(self):
        target_name = str(self._context_menu_target or "").strip()
        handler = self._on_remove_target_request
        if not target_name or not handler:
            return
        confirmed = messagebox.askyesno(
            "删除监听目标",
            f"删除后会写回配置并重启 worker。\n\n确认删除：{target_name}",
            parent=self.root,
        )
        if not confirmed:
            return
        try:
            handler(target_name)
        except Exception as e:
            messagebox.showerror("删除失败", str(e), parent=self.root)

    def switch_chat(self, chat_name: str):
        name = str(chat_name or "").strip()
        if not name:
            return
        if not self._ensure_chat(name):
            return
        self.active_chat = name
        self.unread_counts[name] = 0
        self._update_window_title()
        self._refresh_target_list()
        self._render_active_chat()

    def _insert_message_content(self, msg: SidebarMessage):
        meta_tag = "meta_right" if msg.is_self else "meta_left"
        display_text = msg.text_cn if self.show_original_var.get() and msg.text_cn else (msg.text_display or msg.text_en)
        if msg.pending_translation and not self.show_original_var.get():
            msg_tag = "msg_right_pending" if msg.is_self else "msg_left_pending"
        else:
            msg_tag = "msg_right" if msg.is_self else "msg_left"
        clickable = self._should_render_tts_action(msg, msg.text_en)
        header = f"[{msg.created_at}]"
        if msg.sender_name:
            header += f" {msg.sender_name}"
        self.text.insert(tk.END, header + "\n", meta_tag)
        if clickable:
            body_action_tag = self._register_tts_body_action_tag(msg)
            self.text.insert(tk.END, display_text, (msg_tag, body_action_tag))
        else:
            self.text.insert(tk.END, display_text, msg_tag)
        self.text.insert(tk.END, "\n", msg_tag)

    def _render_active_chat(self):
        self._cancel_pending_tts_body_click()
        self.text.configure(state=tk.NORMAL)
        self.text.delete("1.0", tk.END)
        self._tts_action_tags = {}
        self._tts_action_index = 0
        self._tts_body_click_press = None
        for msg in self.chat_messages.get(self.active_chat, []):
            self._insert_message_content(msg)
        self.text.configure(state=tk.DISABLED)
        self.text.see(tk.END)

    def append_message(self, msg: SidebarMessage):
        if not self._ensure_chat(msg.chat_name):
            return
        cache = self.chat_messages[msg.chat_name]
        append_message_with_limit(cache, msg, self.message_limit)
        if msg.chat_name != self.active_chat:
            self.unread_counts[msg.chat_name] = self.unread_counts.get(msg.chat_name, 0) + 1
            self._refresh_target_list()
            return
        self._render_active_chat()

    def replace_message(self, msg: SidebarMessage) -> bool:
        if not self._ensure_chat(msg.chat_name):
            return False
        cache = self.chat_messages[msg.chat_name]
        replaced = replace_message_in_cache(cache, msg)
        if not replaced:
            return False
        if msg.chat_name == self.active_chat:
            self._render_active_chat()
        return True

    def append_log(self, line: str):
        self.status_var.set(str(line or ""))

    def set_runtime_logger(self, logger: Callable[[str], None] | None):
        self.runtime_logger = logger

    def _emit_runtime_log(self, line: str):
        logger = getattr(self, "runtime_logger", None)
        if not logger:
            return
        try:
            logger(str(line or ""))
        except Exception:
            pass

    def _get_tts_action_block_reason(self, msg: SidebarMessage, display_text: str) -> str:
        if self.show_original_var.get():
            return "original_mode"
        if msg.pending_translation:
            return "pending_translation"
        if not getattr(self, "tts_player", None):
            return "no_player"
        if not is_speakable_english_text(display_text):
            return "non_english_or_invalid"
        return ""

    def _should_render_tts_action(self, msg: SidebarMessage, display_text: str) -> bool:
        return not self._get_tts_action_block_reason(msg, display_text)

    def _play_bound_tts_text(
        self,
        tag_name: str,
        *,
        trigger_name: str,
    ):
        text = self._tts_action_tags.get(str(tag_name or ""))
        if not text:
            self._emit_runtime_log(f"tts {trigger_name} ignored reason=missing_bound_text")
            return "break"
        tts_player = getattr(self, "tts_player", None)
        if not tts_player:
            self._emit_runtime_log(f"tts {trigger_name} ignored reason=no_player")
            return "break"
        result = tts_player.speak_async(text)
        preview = summarize_tts_text(text)
        action = "queued" if result else "rejected"
        self._emit_runtime_log(f"tts {trigger_name} {action} preview={preview}")
        return "break"

    def _register_tts_body_action_tag(self, msg: SidebarMessage) -> str:
        self._tts_action_index += 1
        tag_name = f"tts_body_action_{self._tts_action_index}"
        self._tts_action_tags[tag_name] = str(msg.text_en or "")
        self.text.tag_bind(tag_name, "<ButtonPress-1>", lambda event, tag=tag_name: self._on_tts_body_press(tag, event))
        self.text.tag_bind(tag_name, "<ButtonRelease-1>", lambda event, tag=tag_name: self._on_tts_body_release(tag, event))
        self.text.tag_bind(tag_name, "<Double-Button-1>", self._on_tts_body_multi_click)
        self.text.tag_bind(tag_name, "<Triple-Button-1>", self._on_tts_body_multi_click)
        return tag_name

    def _is_tts_body_tag_hit(self, tag_name: str, event=None) -> bool:
        if event is None:
            return True
        text_widget = getattr(self, "text", None)
        if text_widget is None:
            return False
        try:
            index = text_widget.index(f"@{event.x},{event.y}")
            return tag_name in text_widget.tag_names(index)
        except Exception:
            return False

    def _has_text_selection(self) -> bool:
        text_widget = getattr(self, "text", None)
        if text_widget is None:
            return False
        try:
            return bool(text_widget.tag_ranges(tk.SEL))
        except Exception:
            return False

    def _cancel_pending_tts_body_click(self):
        after_id = str(getattr(self, "_tts_body_click_pending_after_id", "") or "")
        root = getattr(self, "root", None)
        if after_id and root is not None:
            try:
                root.after_cancel(after_id)
            except Exception:
                pass
        self._tts_body_click_pending_after_id = ""
        self._tts_body_click_pending_tag = ""

    def _schedule_tts_body_click_play(self, tag_name: str):
        self._cancel_pending_tts_body_click()
        root = getattr(self, "root", None)
        if root is None:
            self._execute_pending_tts_body_click(tag_name)
            return

        self._tts_body_click_pending_tag = str(tag_name or "")

        def _callback(tag=tag_name):
            self._execute_pending_tts_body_click(tag)

        self._tts_body_click_pending_after_id = str(
            root.after(TTS_BODY_CLICK_PLAY_DELAY_MS, _callback)
        )

    def _execute_pending_tts_body_click(self, tag_name: str):
        pending_tag = str(getattr(self, "_tts_body_click_pending_tag", "") or "")
        self._tts_body_click_pending_after_id = ""
        self._tts_body_click_pending_tag = ""
        if pending_tag and pending_tag != str(tag_name or ""):
            return "break"
        if self._has_text_selection():
            self._emit_runtime_log("tts body click ignored reason=selection")
            return "break"
        return self._play_bound_tts_text(
            tag_name,
            trigger_name="body click",
        )

    def _on_tts_body_press(self, tag_name: str, event=None):
        self._cancel_pending_tts_body_click()
        if not self._is_tts_body_tag_hit(tag_name, event):
            self._tts_body_click_press = None
            return
        self._tts_body_click_press = {
            "tag": str(tag_name or ""),
            "x": int(getattr(event, "x", 0)),
            "y": int(getattr(event, "y", 0)),
        }

    def _on_tts_body_release(self, tag_name: str, event=None):
        press = getattr(self, "_tts_body_click_press", None)
        self._tts_body_click_press = None
        if not isinstance(press, dict):
            return
        expected_tag = str(press.get("tag", "") or "")
        if expected_tag != str(tag_name or ""):
            return
        if not self._is_tts_body_tag_hit(tag_name, event):
            return
        dx = int(getattr(event, "x", 0)) - int(press.get("x", 0))
        dy = int(getattr(event, "y", 0)) - int(press.get("y", 0))
        if dx * dx + dy * dy > TTS_BODY_CLICK_MOVE_TOLERANCE_PX * TTS_BODY_CLICK_MOVE_TOLERANCE_PX:
            return
        self._schedule_tts_body_click_play(tag_name)

    def _on_tts_body_multi_click(self, _event=None):
        self._tts_body_click_press = None
        self._cancel_pending_tts_body_click()
        return

    def maybe_auto_read_message(self, msg: SidebarMessage) -> bool:
        if not self._is_auto_read_enabled():
            return False
        if str(msg.chat_name or "") != str(getattr(self, "active_chat", "")):
            return False
        reason = self._get_tts_action_block_reason(msg, msg.text_en)
        if reason:
            self._emit_runtime_log(
                f"tts auto skipped chat={msg.chat_name} reason={reason} preview={summarize_tts_text(msg.text_en)}"
            )
            return False
        tts_player = getattr(self, "tts_player", None)
        if not tts_player:
            return False
        result = bool(tts_player.speak_async(msg.text_en))
        action = "queued" if result else "rejected"
        self._emit_runtime_log(
            f"tts auto {action} chat={msg.chat_name} preview={summarize_tts_text(msg.text_en)}"
        )
        return result

    def _is_auto_read_enabled(self) -> bool:
        auto_read_var = getattr(self, "tts_auto_read_var", None)
        if auto_read_var is not None:
            try:
                return bool(auto_read_var.get())
            except Exception:
                pass
        return bool(getattr(self, "tts_auto_read_active_chat", True))


def start_worker_process(
    targets: list[str],
    interval: float,
    debug: bool,
    focus_refresh: bool,
    load_retry_seconds: float,
) -> subprocess.Popen:
    # 使用 UTF-8 管道，避免中文在父子进程间错码。
    cmd = build_worker_command(
        targets,
        interval,
        debug,
        focus_refresh,
        load_retry_seconds,
    )
    if is_frozen_app() and not os.path.exists(cmd[0]):
        raise RuntimeError(f"missing worker executable: {cmd[0]}")
    env = os.environ.copy()
    env["PYTHONUTF8"] = "1"
    env["PYTHONIOENCODING"] = "utf-8"
    return subprocess.Popen(
        cmd,
        cwd=ROOT_DIR,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
        bufsize=1,
    )


def stdout_reader(proc: subprocess.Popen, q: "queue.Queue[dict]"):
    assert proc.stdout is not None
    for raw in proc.stdout:
        line = raw.strip()
        if not line:
            continue
        try:
            event = json.loads(line)
            if isinstance(event, dict):
                q.put(event)
            else:
                q.put({"type": "log", "value": f"worker non-dict event: {line}"})
        except json.JSONDecodeError:
            q.put({"type": "log", "value": f"worker raw: {line}"})


def stderr_reader(proc: subprocess.Popen, q: "queue.Queue[dict]"):
    assert proc.stderr is not None
    for raw in proc.stderr:
        line = raw.rstrip()
        if line:
            q.put({"type": "log", "value": f"worker stderr: {line}"})


def main():
    global _TARGET_LOCK_PATHS
    parser = argparse.ArgumentParser(
        description="Sidebar listener: monitor target chats in session-only mode and show translated output."
    )
    parser.add_argument("--config", default=DEFAULT_CONFIG_PATH, help="JSON config path")
    parser.add_argument(
        "--check-tts-deps",
        action="store_true",
        help="Validate packaged TTS Python dependencies and exit",
    )
    parser.add_argument("--target", default="", help=argparse.SUPPRESS)
    args = parser.parse_args()

    config_path = os.path.abspath(args.config)
    try:
        config = load_json_config(config_path)
    except Exception as e:
        exit_startup_error(f"load config failed: {e}")

    config_dir = os.path.dirname(config_path)
    listen_cfg = config.get("listen", {}) if isinstance(config.get("listen", {}), dict) else {}
    translate_cfg = (
        config.get("translate", {}) if isinstance(config.get("translate", {}), dict) else {}
    )
    display_cfg = config.get("display", {}) if isinstance(config.get("display", {}), dict) else {}
    tts_cfg = config.get("tts", {}) if isinstance(config.get("tts", {}), dict) else {}
    logging_cfg = config.get("logging", {}) if isinstance(config.get("logging", {}), dict) else {}

    if args.check_tts_deps:
        try:
            ok, detail = check_tts_dependency_packaging(tts_cfg)
        except RuntimeError as e:
            print(f"[sidebar] tts dependency check failed: {e}", file=sys.stderr)
            raise SystemExit(2)
        stream = sys.stdout if ok else sys.stderr
        print(f"[sidebar] {detail}", file=stream, flush=True)
        raise SystemExit(0 if ok else 2)

    targets = normalize_targets(listen_cfg.get("targets"))
    if not targets:
        exit_startup_error("config listen.targets is empty")
    if str(args.target or "").strip():
        forced_target = str(args.target).strip()
        if forced_target not in targets:
            print(
                f"[sidebar] warning: --target not found in config listen.targets: {forced_target}",
                flush=True,
            )
        targets = [forced_target]

    listen_mode = str(listen_cfg.get("mode", "session")).strip().lower()
    if listen_mode and listen_mode != "session":
        exit_startup_error("session-only branch only supports listen.mode=session")
    focus_refresh = as_bool(listen_cfg.get("focus_refresh"), False)
    worker_debug = as_bool(listen_cfg.get("worker_debug"), False)
    load_retry_seconds = as_non_negative_float(listen_cfg.get("load_retry_seconds"), 10.0)
    session_preview_dedupe_window_seconds = as_non_negative_float(
        listen_cfg.get("session_preview_dedupe_window_seconds"),
        SESSION_PREVIEW_DEDUPE_WINDOW_SECONDS,
    )

    translate_enabled = as_bool(translate_cfg.get("enabled"), True)
    translate_provider = str(translate_cfg.get("provider", "deeplx"))
    deeplx_url = str(translate_cfg.get("deeplx_url") or os.getenv("DEEPLX_URL", "")).strip()
    source_lang = str(translate_cfg.get("source_lang", "auto"))
    target_lang = str(translate_cfg.get("target_lang", "EN"))

    english_only = as_bool(display_cfg.get("english_only"), True)
    tts_auto_read_active_chat = as_bool(display_cfg.get("tts_auto_read_active_chat"), True)
    translate_fail_behavior = str(display_cfg.get("on_translate_fail", "show_cn_with_reason"))
    if translate_fail_behavior not in ("show_cn_with_reason", "show_cn", "show_reason"):
        translate_fail_behavior = "show_cn_with_reason"
    side = str(display_cfg.get("side", "right"))
    if side not in ("left", "right"):
        side = "right"

    try:
        listen_interval = read_config_float(
            listen_cfg,
            "interval_seconds",
            DEFAULT_LISTEN_INTERVAL_SECONDS,
        )
        translate_timeout = read_config_float(translate_cfg, "timeout_seconds", 8.0)
        width = read_config_int(display_cfg, "width", DEFAULT_SIDEBAR_WIDTH)
        listen_interval = validate_positive_float("listen.interval_seconds", listen_interval)
        listen_interval = validate_float_min(
            "listen.interval_seconds",
            listen_interval,
            MIN_LISTEN_INTERVAL_SECONDS,
        )
        translate_timeout = validate_positive_float("translate.timeout_seconds", translate_timeout)
        width = validate_int_min("display.width", width, MIN_SIDEBAR_WIDTH)
        load_retry_seconds = validate_positive_float(
            "listen.load_retry_seconds",
            load_retry_seconds,
        )
        translate_provider = normalize_translate_provider(translate_provider)
        validate_translate_config(translate_enabled, translate_provider, deeplx_url)
        translator = create_translator(
            enabled=translate_enabled,
            provider=translate_provider,
            deeplx_url=deeplx_url,
            source_lang=source_lang,
            target_lang=target_lang,
            timeout_seconds=translate_timeout,
        )
        translator_runtime_text = build_translator_runtime_text(
            translate_enabled,
            translate_provider,
        )
        tts_player, tts_runtime_text = create_tts_player(
            tts_cfg,
            config_dir=config_dir,
        )
    except RuntimeError as e:
        exit_startup_error(f"invalid config: {e}")

    log_file = resolve_log_file_path(logging_cfg.get("file", ""))
    cleaned_stale_locks = cleanup_stale_target_locks()
    if cleaned_stale_locks > 0:
        print(f"[sidebar] cleaned stale locks: {cleaned_stale_locks}", flush=True)
        append_log_file(log_file, f"cleaned stale locks: {cleaned_stale_locks}")

    running_targets: list[str] = []
    running_target_locks: dict[str, str] = {}
    skipped_targets: list[tuple[str, str]] = []
    for target in targets:
        locked, lock_info = acquire_target_lock(target)
        if not locked:
            skipped_targets.append((target, lock_info))
            print(f"[sidebar] skip target={target}: {lock_info}", flush=True)
            continue
        running_targets.append(target)
        running_target_locks[target] = lock_info
    if not running_targets:
        print("[sidebar] no available targets to start", flush=True)
        return
    _TARGET_LOCK_PATHS = list(running_target_locks.values())
    atexit.register(release_target_lock)

    ui = SidebarUI(
        width=width,
        side=side,
        targets=running_targets,
        message_limit=CHAT_CACHE_LIMIT,
        tts_player=tts_player,
        tts_auto_read_active_chat=tts_auto_read_active_chat,
    )
    for target, reason in skipped_targets:
        append_log_file(log_file, f"skip target={target}: {reason}")

    append_log_file(log_file, "sidebar start")
    append_log_file(log_file, f"requested targets={targets}")
    append_log_file(log_file, f"running targets={running_targets}")
    append_log_file(log_file, translator_runtime_text)
    append_log_file(log_file, tts_runtime_text)
    append_log_file(log_file, f"tts auto read active chat={tts_auto_read_active_chat}")
    append_log_file(log_file, f"load retry seconds={load_retry_seconds}")
    append_log_file(log_file, f"chat cache limit={CHAT_CACHE_LIMIT}")
    append_log_file(log_file, f"session preview dedupe window={session_preview_dedupe_window_seconds}s")
    event_queue: "queue.Queue[dict]" = queue.Queue()
    runtime_log = lambda line: event_queue.put({"type": "log", "value": str(line or "")})
    if tts_player and hasattr(tts_player, "set_logger"):
        tts_player.set_logger(runtime_log)
    ui.set_runtime_logger(runtime_log)
    translate_queue: "queue.Queue[dict | None]" = queue.Queue(maxsize=TRANSLATE_QUEUE_MAXSIZE)
    dedupe_cache: Dict[str, float] = {}
    last_dedupe_cleanup_at = 0.0
    translate_queue_drop_count = 0
    last_translate_queue_drop_log_at = 0.0
    next_message_sequence = 0
    worker: subprocess.Popen | None = None
    worker_last_handled_exit_pid = 0
    worker_restart_attempt = 0
    worker_restart_deadline = 0.0
    worker_stop_deadline = 0.0
    worker_force_kill_requested = False
    worker_state = "starting"
    worker_detail = "starting worker"
    pending_worker_launch_reason = ""
    closing = False
    ui.set_status(build_worker_status_text(worker_state, worker_detail, len(running_targets)))

    def sync_target_lock_paths():
        global _TARGET_LOCK_PATHS
        _TARGET_LOCK_PATHS = list(running_target_locks.values())

    def persist_targets_to_config(next_targets: list[str]):
        save_listener_targets_config(config_path, config, next_targets)

    def clear_target_runtime_state(target_name: str):
        prefix = f"{target_name}::"
        stale_keys = [key for key in dedupe_cache if key.startswith(prefix)]
        for key in stale_keys:
            dedupe_cache.pop(key, None)

    def schedule_worker_restart(reason: str, attempt: int):
        nonlocal worker_restart_attempt, worker_restart_deadline, worker_state, worker_detail
        nonlocal worker_stop_deadline, worker_force_kill_requested
        delay = compute_worker_restart_delay(attempt)
        worker_restart_attempt = attempt
        worker_restart_deadline = time.time() + delay
        worker_stop_deadline = 0.0
        worker_force_kill_requested = False
        worker_state = "worker_backoff"
        worker_detail = f"{reason}, retry in {delay:.1f}s"
        backoff_line = f"worker backoff attempt={attempt} delay={delay:.1f}s reason={reason}"
        ui.append_log(backoff_line)
        append_log_file(log_file, backoff_line)

    def launch_worker(reason: str) -> bool:
        nonlocal worker, worker_last_handled_exit_pid, worker_restart_deadline, worker_state, worker_detail
        nonlocal worker_stop_deadline, worker_force_kill_requested
        try:
            worker = start_worker_process(
                running_targets,
                listen_interval,
                worker_debug,
                focus_refresh,
                load_retry_seconds,
            )
        except Exception as e:
            attempt = worker_restart_attempt + 1
            schedule_worker_restart(f"{reason} failed: {e}", attempt)
            return False

        worker_last_handled_exit_pid = 0
        worker_restart_deadline = 0.0
        worker_stop_deadline = 0.0
        worker_force_kill_requested = False
        worker_state = "starting"
        worker_detail = reason
        append_log_file(log_file, f"worker start pid={worker.pid} reason={reason}")
        t_out = threading.Thread(target=stdout_reader, args=(worker, event_queue), daemon=True)
        t_err = threading.Thread(target=stderr_reader, args=(worker, event_queue), daemon=True)
        t_out.start()
        t_err.start()
        return True

    def request_worker_restart(reason: str):
        nonlocal worker, worker_restart_attempt, worker_restart_deadline
        nonlocal worker_state, worker_detail, pending_worker_launch_reason
        nonlocal worker_stop_deadline, worker_force_kill_requested
        restart_reason = str(reason or "targets changed").strip() or "targets changed"
        pending_worker_launch_reason = restart_reason
        worker_restart_attempt = 0
        worker_restart_deadline = 0.0
        worker_state = "restarting"
        worker_detail = restart_reason
        append_log_file(log_file, f"worker restarting: {restart_reason}")
        if worker is None:
            worker_stop_deadline = 0.0
            worker_force_kill_requested = False
            launch_reason = pending_worker_launch_reason
            pending_worker_launch_reason = ""
            launch_worker(launch_reason)
            return
        if worker.poll() is not None:
            worker = None
            worker_stop_deadline = 0.0
            worker_force_kill_requested = False
            launch_reason = pending_worker_launch_reason
            pending_worker_launch_reason = ""
            launch_worker(launch_reason)
            return
        if worker_stop_deadline:
            worker_detail = f"{restart_reason}, waiting previous worker exit"
            return
        try:
            worker.terminate()
            worker_stop_deadline = time.time() + WORKER_STOP_TIMEOUT_SECONDS
            worker_force_kill_requested = False
            append_log_file(
                log_file,
                f"worker terminate requested pid={worker.pid} reason={restart_reason}",
            )
        except Exception:
            try:
                worker.kill()
                worker_stop_deadline = time.time() + WORKER_FORCE_KILL_TIMEOUT_SECONDS
                worker_force_kill_requested = True
                append_log_file(
                    log_file,
                    f"worker kill requested pid={worker.pid} reason={restart_reason}",
                )
            except Exception as e:
                pending_worker_launch_reason = ""
                worker_stop_deadline = 0.0
                worker_force_kill_requested = False
                raise RuntimeError(f"stop worker failed: {e}")

    def handle_add_target_request(raw_target: str):
        target_name = str(raw_target or "").strip()
        if not target_name:
            raise RuntimeError("会话名不能为空")
        if target_name in running_targets:
            raise RuntimeError(f"监听目标已存在：{target_name}")

        locked, lock_info = acquire_target_lock(target_name)
        if not locked:
            raise RuntimeError(f"无法添加：{lock_info}")

        running_targets.append(target_name)
        running_target_locks[target_name] = lock_info
        sync_target_lock_paths()
        ui.add_target(target_name)
        try:
            persist_targets_to_config(running_targets)
        except Exception as e:
            ui.remove_target(target_name)
            if target_name in running_targets:
                running_targets.remove(target_name)
            lock_path = running_target_locks.pop(target_name, "")
            sync_target_lock_paths()
            if lock_path:
                release_target_lock_path(lock_path)
            raise RuntimeError(f"写回配置失败：{e}")

        clear_target_runtime_state(target_name)
        change_line = f"target added: {target_name}"
        ui.append_log(change_line)
        append_log_file(log_file, change_line)
        request_worker_restart(f"targets updated after add: {target_name}")

    def handle_remove_target_request(raw_target: str):
        target_name = str(raw_target or "").strip()
        if not target_name:
            raise RuntimeError("会话名不能为空")
        if target_name not in running_targets:
            raise RuntimeError(f"监听目标不存在：{target_name}")
        if len(running_targets) <= 1:
            raise RuntimeError("至少保留 1 个监听目标；最后一个不允许直接删除")

        next_targets = [item for item in running_targets if item != target_name]
        try:
            persist_targets_to_config(next_targets)
        except Exception as e:
            raise RuntimeError(f"写回配置失败：{e}")

        running_targets[:] = next_targets
        lock_path = running_target_locks.pop(target_name, "")
        sync_target_lock_paths()
        if lock_path:
            release_target_lock_path(lock_path)
        clear_target_runtime_state(target_name)
        ui.remove_target(target_name)
        change_line = f"target removed: {target_name}"
        ui.append_log(change_line)
        append_log_file(log_file, change_line)
        request_worker_restart(f"targets updated after remove: {target_name}")

    ui.set_target_editor_handlers(
        handle_add_target_request,
        handle_remove_target_request,
    )

    launch_worker("starting worker")

    def build_pending_message(
        chat_name: str,
        sender_name: str,
        body_cn: str,
        created_at: str,
        is_self: bool,
        message_id: str,
    ) -> SidebarMessage:
        return SidebarMessage(
            chat_name=chat_name,
            sender_name=sender_name if not is_self else "",
            text_en=TRANSLATE_PENDING_TEXT,
            text_display=TRANSLATE_PENDING_TEXT,
            text_cn=body_cn,
            created_at=created_at,
            is_self=is_self,
            message_id=message_id,
            pending_translation=True,
        )

    def emit_translate_result(
        task: dict,
        text_en: str,
        text_display: str,
        pending_translation: bool = False,
    ):
        event_queue.put(
            {
                "type": "render_message",
                "message_id": str(task.get("message_id", "")),
                "chat_name": task["chat_name"],
                "sender_name": task["sender_name"],
                "is_self": task["is_self"],
                "source": task["source"],
                "created_at": task["created_at"],
                "text_cn": str(task.get("body_cn", "")),
                "text_en": text_en,
                "text_display": text_display,
                "pending_translation": pending_translation,
            }
        )

    def resolve_dropped_translate_task(task: dict):
        if not isinstance(task, dict):
            return
        body_cn = str(task.get("body_cn", ""))
        fallback_text = build_translate_fallback(
            body_cn,
            RuntimeError("translate queue overflow"),
            translate_fail_behavior,
        )
        emit_translate_result(task, fallback_text, fallback_text)

    def enqueue_translate_task(task: dict):
        nonlocal translate_queue_drop_count, last_translate_queue_drop_log_at
        try:
            translate_queue.put_nowait(task)
            return
        except queue.Full:
            pass

        empty_marker = object()
        dropped = empty_marker
        try:
            dropped = translate_queue.get_nowait()
        except queue.Empty:
            pass

        # 队列里如果是停止标记，恢复标记并放弃当前消息，优先保证退出流程可达。
        if dropped is None:
            try:
                translate_queue.put_nowait(None)
            except queue.Full:
                pass
            translate_queue_drop_count += 1
            resolve_dropped_translate_task(task)
        else:
            resolve_dropped_translate_task(dropped)
            try:
                translate_queue.put_nowait(task)
                return
            except queue.Full:
                translate_queue_drop_count += 1
                resolve_dropped_translate_task(task)

        now_ts = time.time()
        if (
            now_ts - last_translate_queue_drop_log_at
            >= TRANSLATE_QUEUE_DROP_LOG_INTERVAL_SECONDS
        ):
            event_queue.put(
                {
                    "type": "log",
                    "value": f"translate queue overflow: dropped={translate_queue_drop_count}",
                }
            )
            last_translate_queue_drop_log_at = now_ts

    def signal_translate_worker_stop():
        try:
            translate_queue.put_nowait(None)
            return
        except queue.Full:
            pass

        try:
            translate_queue.get_nowait()
        except queue.Empty:
            pass
        try:
            translate_queue.put_nowait(None)
        except queue.Full:
            pass

    def translate_worker():
        while True:
            task = translate_queue.get()
            if task is None:
                return

            body_cn = str(task.get("body_cn", ""))
            rendered_body = body_cn
            if translate_enabled:
                try:
                    rendered_body = translator.translate(body_cn)
                except Exception as e:
                    rendered_body = build_translate_fallback(
                        body_cn, e, translate_fail_behavior
                    )
                    event_queue.put({"type": "log", "value": f"translate fallback: {e}"})

            rendered_text = rendered_body
            if not english_only and rendered_body != body_cn:
                rendered_text = f"{rendered_text}\nCN: {body_cn}"

            emit_translate_result(task, rendered_body, rendered_text)

    t_translate = threading.Thread(target=translate_worker, daemon=True)
    t_translate.start()

    def handle_event(event: dict):
        nonlocal last_dedupe_cleanup_at, worker_restart_attempt, worker_restart_deadline
        nonlocal worker_state, worker_detail
        nonlocal next_message_sequence
        kind = event.get("type")
        default_chat_name = running_targets[0] if running_targets else ""
        if kind == "status":
            worker_state = str(event.get("state", "status")).strip() or "status"
            worker_detail = str(event.get("value", ""))
            if worker_state == "running":
                worker_restart_attempt = 0
                worker_restart_deadline = 0.0
            ui.append_log(f"status: {worker_detail}")
            append_log_file(log_file, f"status: {worker_detail}")
            return

        if kind == "log":
            value = str(event.get("value", ""))
            ui.append_log(value)
            append_log_file(log_file, value)
            return

        if kind == "render_message":
            created_at = str(event.get("created_at") or datetime.now().strftime("%H:%M:%S"))
            chat_name = str(event.get("chat_name") or default_chat_name)
            sender_name = str(event.get("sender_name", ""))
            raw_text = str(event.get("text_cn", ""))
            rendered_text = str(event.get("text_en", ""))
            display_text = str(event.get("text_display", rendered_text))
            source = str(event.get("source", ""))
            is_self = bool(event.get("is_self", False))
            message_id = str(event.get("message_id", ""))
            pending_translation = bool(event.get("pending_translation", False))
            msg = SidebarMessage(
                chat_name=chat_name,
                sender_name=sender_name if not is_self else "",
                text_en=rendered_text,
                text_display=display_text,
                text_cn=raw_text,
                created_at=created_at,
                is_self=is_self,
                message_id=message_id,
                pending_translation=pending_translation,
            )
            if not ui.replace_message(msg):
                ui.append_message(msg)
            ui.maybe_auto_read_message(msg)
            append_log_file(
                log_file,
                (
                    f"[{created_at}] chat={chat_name} source={source} sender={sender_name or 'self'} "
                    f"en={rendered_text}"
                ),
            )
            return

        if kind != "message":
            ui.append_log(f"unknown event: {event}")
            append_log_file(log_file, f"unknown event: {event}")
            return

        chat_name = str(event.get("chat_name") or default_chat_name)
        if not chat_name:
            return
        cn_text = str(event.get("text", "")).strip()
        if not cn_text:
            return

        # 1) 先拆发送人和正文（发送人姓名不翻译）。
        sender_name, body_cn, is_self = split_sender_and_body(cn_text)
        if not body_cn:
            return
        # 2) 明显的媒体占位消息直接过滤（例如 [图片] / [视频] / [动画表情] / [语音]）。
        if is_filtered_placeholder(body_cn):
            append_log_file(log_file, f"skip filtered placeholder: {chat_name} {cn_text}")
            return
        # 3) 带显式 http/https 链接的消息直接过滤，不显示也不翻译。
        if is_filtered_link_message(body_cn):
            append_log_file(log_file, f"skip filtered link: {chat_name} {cn_text}")
            return

        normalized_body = normalize_message_for_dedupe(body_cn)
        if not normalized_body:
            return

        now_ts = time.time()

        dedupe_key = f"{chat_name}::{sender_name}::{normalized_body}"
        prev_ts = dedupe_cache.get(dedupe_key)
        if prev_ts is not None and now_ts - prev_ts <= session_preview_dedupe_window_seconds:
            return

        dedupe_cache[dedupe_key] = now_ts

        if now_ts - last_dedupe_cleanup_at >= DEDUPE_CLEANUP_INTERVAL_SECONDS:
            cleanup_dedupe_cache(dedupe_cache, now_ts)
            last_dedupe_cleanup_at = now_ts

        created_at = str(event.get("created_at") or datetime.now().strftime("%H:%M:%S"))
        message_id = f"msg-{next_message_sequence}"
        next_message_sequence += 1

        if translate_enabled and translate_provider == "deeplx":
            ui.append_message(
                build_pending_message(
                    chat_name=chat_name,
                    sender_name=sender_name,
                    body_cn=body_cn,
                    created_at=created_at,
                    is_self=is_self,
                    message_id=message_id,
                )
            )

        enqueue_translate_task(
            {
                "message_id": message_id,
                "chat_name": chat_name,
                "sender_name": sender_name,
                "body_cn": body_cn,
                "is_self": is_self,
                "source": "session_preview",
                "created_at": created_at,
            }
        )

    def drain_queue():
        nonlocal worker, worker_last_handled_exit_pid, worker_state, worker_detail
        nonlocal pending_worker_launch_reason
        nonlocal worker_stop_deadline, worker_force_kill_requested
        try:
            while True:
                event = event_queue.get_nowait()
                handle_event(event)
        except queue.Empty:
            pass

        now_ts = time.time()
        if worker is not None:
            return_code = worker.poll()
            if return_code is not None and worker_last_handled_exit_pid != worker.pid:
                worker_last_handled_exit_pid = worker.pid
                worker = None
                worker_stop_deadline = 0.0
                worker_force_kill_requested = False
                exit_line = f"worker exited code={return_code}"
                ui.append_log(exit_line)
                append_log_file(log_file, exit_line)
                if closing:
                    worker_state = "stopped"
                    worker_detail = f"code={return_code}"
                elif pending_worker_launch_reason:
                    restart_reason = pending_worker_launch_reason
                    pending_worker_launch_reason = ""
                    launch_worker(restart_reason)
                else:
                    attempt = worker_restart_attempt + 1
                    schedule_worker_restart(f"code={return_code}", attempt)
            elif not closing and pending_worker_launch_reason and worker_stop_deadline and now_ts >= worker_stop_deadline:
                restart_reason = pending_worker_launch_reason
                if not worker_force_kill_requested:
                    try:
                        worker.kill()
                        worker_force_kill_requested = True
                        worker_stop_deadline = now_ts + WORKER_FORCE_KILL_TIMEOUT_SECONDS
                        worker_state = "restarting"
                        worker_detail = f"{restart_reason}, forcing previous worker exit"
                        kill_line = f"worker kill requested pid={worker.pid} reason={restart_reason}"
                        ui.append_log(kill_line)
                        append_log_file(log_file, kill_line)
                    except Exception as e:
                        worker_stop_deadline = now_ts + WORKER_FORCE_KILL_TIMEOUT_SECONDS
                        fail_line = f"worker kill request failed pid={worker.pid} reason={restart_reason} error={e}"
                        ui.append_log(fail_line)
                        append_log_file(log_file, fail_line)
                else:
                    worker_stop_deadline = now_ts + WORKER_FORCE_KILL_TIMEOUT_SECONDS
                    worker_state = "restarting"
                    worker_detail = f"{restart_reason}, waiting previous worker exit"

        if not closing and worker_restart_deadline and now_ts >= worker_restart_deadline:
            launch_worker(f"restarting worker attempt={worker_restart_attempt}")

        ui.set_status(build_worker_status_text(worker_state, worker_detail, len(running_targets)))

        ui.root.after(UI_DRAIN_INTERVAL_MS, drain_queue)

    def on_close():
        nonlocal closing, worker_restart_deadline
        closing = True
        worker_restart_deadline = 0.0
        try:
            ui._cancel_pending_tts_body_click()
        except Exception:
            pass
        try:
            if worker is not None and worker.poll() is None:
                worker.terminate()
        except Exception:
            pass
        try:
            signal_translate_worker_stop()
        except Exception:
            pass
        release_target_lock()
        ui.root.destroy()

    ui.root.protocol("WM_DELETE_WINDOW", on_close)
    ui.root.after(UI_DRAIN_INTERVAL_MS, drain_queue)
    ui.root.mainloop()


if __name__ == "__main__":
    main()
