import { useState, useEffect, useCallback, useRef } from "react"
import type { ShortcutModifiers } from "../components/SettingsApp"
import {
  formatKeyWithModifiers,
  getKeyDisplayName,
  hasAnyModifier,
  setShortcutPaused,
  shortcutKeyFromCode,
} from "../components/keyRecording"

interface UseKeyRecordingOptions {
  key: string
  modifiers: ShortcutModifiers
  onKeyRecorded: (key: string, modifiers: ShortcutModifiers) => void
}

interface UseKeyRecordingResult {
  isRecording: boolean
  displayName: string | null
  handleRecordKey: () => void
  handleCancelRecord: () => void
}

const MODIFIER_KEYS = new Set(["Shift", "Control", "Alt", "Meta", "CapsLock", "Fn"])

export function useKeyRecording({
  key,
  modifiers,
  onKeyRecorded,
}: UseKeyRecordingOptions): UseKeyRecordingResult {
  const [isRecording, setIsRecording] = useState(false)
  const [displayName, setDisplayName] = useState<string | null>(null)
  const onKeyRecordedRef = useRef(onKeyRecorded)
  onKeyRecordedRef.current = onKeyRecorded

  useEffect(() => {
    getKeyDisplayName(key)
      .then((name) => {
        if (name) {
          setDisplayName(formatKeyWithModifiers(name, modifiers))
        } else {
          setDisplayName(null)
        }
      })
      .catch(() => setDisplayName(null))
  }, [key, modifiers])

  useEffect(() => {
    if (!isRecording) return

    let active = true
    setShortcutPaused(true).catch((e) => console.error("Failed to pause shortcut:", e))

    const handleKeyDown = (event: KeyboardEvent) => {
      event.preventDefault()
      event.stopPropagation()
      if (event.repeat || MODIFIER_KEYS.has(event.key)) return

      const pressed: ShortcutModifiers = {
        shift: event.shiftKey,
        control: event.ctrlKey,
        option: event.altKey,
        command: event.metaKey,
      }
      if (event.code === "Escape" && !hasAnyModifier(pressed)) {
        setIsRecording(false)
        return
      }

      shortcutKeyFromCode(event.code)
        .then((recorded) => {
          if (!active || !recorded) return
          // A global shortcut without modifiers would swallow that key in every app.
          if (!hasAnyModifier(pressed) && !/^f\d+$/.test(recorded.name)) return
          onKeyRecordedRef.current(recorded.name, pressed)
          setIsRecording(false)
        })
        .catch((e) => console.error("Failed to record key:", e))
    }
    const handleBlur = () => setIsRecording(false)

    window.addEventListener("keydown", handleKeyDown, true)
    window.addEventListener("blur", handleBlur)
    return () => {
      active = false
      window.removeEventListener("keydown", handleKeyDown, true)
      window.removeEventListener("blur", handleBlur)
      setShortcutPaused(false).catch((e) => console.error("Failed to resume shortcut:", e))
    }
  }, [isRecording])

  const handleRecordKey = useCallback(() => setIsRecording(true), [])
  const handleCancelRecord = useCallback(() => setIsRecording(false), [])

  return {
    isRecording,
    displayName,
    handleRecordKey,
    handleCancelRecord,
  }
}
