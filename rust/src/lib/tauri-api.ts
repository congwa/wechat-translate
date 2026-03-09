import { invoke } from "@tauri-apps/api/core";
import type { ApiResponse } from "./types";

export async function sendText(who: string, text: string): Promise<ApiResponse> {
  return invoke("send_text", { who, text });
}

export async function sendFiles(who: string, filePaths: string[]): Promise<ApiResponse> {
  return invoke("send_files", { who, filePaths });
}

export async function getSessions(): Promise<ApiResponse<string[]>> {
  return invoke("get_sessions");
}

export async function listenStart(intervalSeconds?: number): Promise<ApiResponse> {
  return invoke("listen_start", { intervalSeconds });
}

export async function listenStop(): Promise<ApiResponse> {
  return invoke("listen_stop");
}

export async function autoreplyStart(): Promise<ApiResponse> {
  return invoke("autoreply_start");
}

export async function autoreplyStop(): Promise<ApiResponse> {
  return invoke("autoreply_stop");
}

export async function sidebarStart(params: {
  targets?: string[];
  translateEnabled?: boolean;
  deeplxUrl?: string;
  sourceLang?: string;
  targetLang?: string;
  timeoutSeconds?: number;
  betaImageCapture?: boolean;
  betaAvatarCapture?: boolean;
}): Promise<ApiResponse> {
  return invoke("sidebar_start", params);
}

export async function sidebarStop(): Promise<ApiResponse> {
  return invoke("sidebar_stop");
}

export async function liveStart(params?: {
  translateEnabled?: boolean;
  deeplxUrl?: string;
  sourceLang?: string;
  targetLang?: string;
  intervalSeconds?: number;
  betaImageCapture?: boolean;
  betaAvatarCapture?: boolean;
  windowMode?: string;
}): Promise<ApiResponse> {
  return invoke("live_start", params ?? {});
}

export async function sidebarWindowOpen(width?: number, windowMode?: string): Promise<ApiResponse> {
  return invoke("sidebar_window_open", { width, windowMode });
}

export async function sidebarWindowClose(): Promise<ApiResponse> {
  return invoke("sidebar_window_close");
}

export async function getTaskStatus(): Promise<ApiResponse> {
  return invoke("get_task_status");
}

export async function healthCheck(): Promise<ApiResponse> {
  return invoke("health_check");
}

export async function configGet(): Promise<ApiResponse> {
  return invoke("config_get");
}

export interface ConfigPutResponse {
  ok: boolean;
  errors?: string[];
  message?: string;
  path?: string;
}

export async function configPut(config: unknown): Promise<ConfigPutResponse> {
  return invoke("config_put", { config });
}

export async function configDefault(): Promise<ApiResponse> {
  return invoke("config_default");
}

export async function dbClearRestart(): Promise<ApiResponse> {
  return invoke("db_clear_restart");
}

export async function dbQueryMessages(params: {
  chatName?: string;
  sender?: string;
  keyword?: string;
  limit?: number;
  offset?: number;
}): Promise<ApiResponse> {
  return invoke("db_query_messages", params);
}

export async function dbGetChats(): Promise<ApiResponse> {
  return invoke("db_get_chats");
}

export async function dbGetStats(): Promise<ApiResponse> {
  return invoke("db_get_stats");
}

export async function getCloseToTray(): Promise<boolean> {
  return invoke("get_close_to_tray");
}

export async function setCloseToTray(enabled: boolean): Promise<void> {
  return invoke("set_close_to_tray", { enabled });
}

export interface PreflightResult {
  wechat_running: boolean;
  accessibility_ok: boolean;
  wechat_has_window: boolean;
  can_prompt_accessibility?: boolean;
}

export async function preflightCheck(): Promise<PreflightResult> {
  return invoke("preflight_check");
}

export interface AccessibilityRequestResult {
  trusted_before: boolean;
  prompt_attempted: boolean;
  trusted_after_check: boolean;
  settings_opened: boolean;
}

export interface AccessibilityOpenSettingsResult {
  ok: boolean;
  settings_opened: boolean;
  message?: string;
}

export async function accessibilityRequestAccess(): Promise<AccessibilityRequestResult> {
  return invoke("accessibility_request_access");
}

export async function accessibilityOpenSettings(): Promise<AccessibilityOpenSettingsResult> {
  return invoke("accessibility_open_settings");
}

export async function translateTest(params: {
  deeplxUrl: string;
  sourceLang?: string;
  targetLang?: string;
  timeoutSeconds?: number;
}): Promise<ApiResponse> {
  return invoke("translate_test", params);
}
