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
