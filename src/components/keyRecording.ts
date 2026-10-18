import { invoke } from "@tauri-apps/api/core"
import type { ShortcutModifiers } from "./SettingsApp"

export interface ShortcutKey {
  name: string
  display_name: string
}

export function formatKeyWithModifiers(
  displayName: string,
  modifiers: ShortcutModifiers,
): string {
  const parts: string[] = []
  if (modifiers.control) parts.push("Ctrl")
  if (modifiers.option) parts.push("Opt")
  if (modifiers.shift) parts.push("Shift")
  if (modifiers.command) parts.push("Cmd")
  parts.push(displayName)
  return parts.join(" + ")
}

export function hasAnyModifier(modifiers: ShortcutModifiers): boolean {
  return modifiers.shift || modifiers.control || modifiers.option || modifiers.command
}

export async function getKeyDisplayName(keyName: string): Promise<string | null> {
  return invoke<string | null>("get_key_display_name", { keyName })
}

export async function shortcutKeyFromCode(code: string): Promise<ShortcutKey | null> {
  return invoke<ShortcutKey | null>("shortcut_key_from_code", { code })
}

export async function setShortcutPaused(paused: boolean): Promise<void> {
  await invoke("set_shortcut_paused", { paused })
}

export async function getShortcutError(): Promise<string | null> {
  return invoke<string | null>("get_shortcut_error")
}
