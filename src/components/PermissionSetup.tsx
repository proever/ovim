import { useEffect, useState } from "react"
import { invoke } from "@tauri-apps/api/core"

export type PermissionStatus = {
  accessibility: boolean
}

type PermissionSetupProps = {
  onboarding?: boolean
}

export let PermissionSetup = ({ onboarding = false }: PermissionSetupProps) => {
  let [permissionStatus, setPermissionStatus] = useState<PermissionStatus | null>(null)
  let [completionStarted, setCompletionStarted] = useState(false)
  let [errorMessage, setErrorMessage] = useState("")

  useEffect(() => {
    let checkPermissions = () => {
      invoke<PermissionStatus>("get_permission_status")
        .then(setPermissionStatus)
        .catch((error) => setErrorMessage(String(error)))
    }

    checkPermissions()
    let interval = window.setInterval(checkPermissions, 1000)
    return () => window.clearInterval(interval)
  }, [])

  useEffect(() => {
    if (!onboarding || completionStarted || !permissionStatus?.accessibility) {
      return
    }

    setCompletionStarted(true)
    invoke("complete_permission_setup").catch((error) => {
      setCompletionStarted(false)
      setErrorMessage(String(error))
    })
  }, [completionStarted, onboarding, permissionStatus])

  let handleAllowAccessibility = () => {
    setErrorMessage("")
    invoke("open_accessibility_settings").catch((error) => setErrorMessage(String(error)))
  }

  if (!permissionStatus) {
    return <div className="permission-loading">Checking permissions...</div>
  }

  return (
    <div className={`permission-setup ${onboarding ? "permission-setup-onboarding" : ""}`}>
      <div className="permission-card">
        <div className="permission-copy">
          <div className="permission-name">Accessibility</div>
          <div className="permission-description">
            Lets ovim read the focused text field and paste your edits back into it.
          </div>
        </div>
        {permissionStatus.accessibility ? (
          <>
            <span className="permission-allowed">Allowed</span>
            <button type="button" className="permission-allow" onClick={handleAllowAccessibility}>
              Settings
            </button>
          </>
        ) : (
          <button type="button" className="permission-allow" onClick={handleAllowAccessibility}>
            Allow
          </button>
        )}
      </div>

      {errorMessage && <div className="permission-error">{errorMessage}</div>}
    </div>
  )
}
