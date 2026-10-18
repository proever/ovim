import { PermissionSetup } from "./PermissionSetup"
import "../permissions.css"

export let PermissionsApp = () => (
  <main className="permissions-window">
    <div className="permissions-mark">OV</div>
    <h1>Allow ovim to control your Mac</h1>
    <p className="permissions-intro">
      ovim needs Accessibility access to read the focused text field and paste your edits back.
    </p>
    <PermissionSetup onboarding />
    <p className="permissions-footnote">
      Allow opens the correct System Settings page. Drag the ovim icon from the helper into the app list if needed.
    </p>
  </main>
)
