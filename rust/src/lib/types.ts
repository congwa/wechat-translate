export interface ServiceEvent {
  id: number;
  type: EventType;
  source: string;
  timestamp: string;
  payload: Record<string, unknown>;
}

export type EventType = "status" | "message" | "log" | "error" | "task_state";

export interface TaskState {
  monitoring: boolean;
  sidebar: boolean;
}

export interface TranslatorServiceStatus {
  enabled: boolean;
  configured: boolean;
  checking: boolean;
  healthy: boolean | null;
  last_error: string | null;
}

export interface ListenSettings {
  mode: string;
  targets: string[];
  interval_seconds: number;
  dedupe_window_seconds: number;
  session_preview_dedupe_window_seconds: number;
  cross_source_merge_window_seconds: number;
  focus_refresh: boolean;
  worker_debug: boolean;
  use_right_panel_details: boolean;
}

export interface TranslateSettings {
  enabled: boolean;
  provider: string;
  deeplx_url: string;
  source_lang: string;
  target_lang: string;
  timeout_seconds: number;
  max_concurrency: number;
  max_requests_per_second: number;
}

export interface DisplaySettings {
  english_only: boolean;
  on_translate_fail: string;
  width: number;
  side: string;
}

export interface LoggingSettings {
  file: string;
}

export interface AppSettings {
  listen: ListenSettings;
  translate: TranslateSettings;
  display: DisplaySettings;
  logging: LoggingSettings;
}

export interface AppRuntime {
  tasks: TaskState;
  translator: TranslatorServiceStatus;
  close_to_tray: boolean;
}

export interface AppStateSnapshot {
  settings: AppSettings;
  runtime: AppRuntime;
}

export interface ServiceStatus {
  adapter: {
    platform: string;
    supported: boolean;
    reason: string;
  };
  tasks: TaskState;
  translator: TranslatorServiceStatus;
}

export interface ApiResponse<T = unknown> {
  ok: boolean;
  message?: string;
  data?: T;
}

export interface SidebarMessage {
  id: number;
  chatName: string;
  sender: string;
  textCn: string;
  textEn: string;
  translateError: string;
  timestamp: string;
  isSelf: boolean;
  imagePath?: string;
}

export interface StoredMessage {
  id: number;
  chat_name: string;
  sender: string;
  content: string;
  content_en: string;
  is_self: boolean;
  detected_at: string;
  image_path?: string;
  source?: string;
  quality?: string;
}

export interface SidebarSnapshot {
  current_chat?: string | null;
  messages: StoredMessage[];
  translator: TranslatorServiceStatus;
}
