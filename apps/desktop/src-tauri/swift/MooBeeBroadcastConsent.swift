// Native OS confirmation only. Never invoke the publisher or access credentials.
import AppKit

@_cdecl("moobee_confirm_test_broadcast")
public func moobeeConfirmTestBroadcast(_ message: UnsafePointer<CChar>?) -> Int32 {
    guard Thread.isMainThread, let message = message,
          let detail = String(validatingUTF8: message) else { return 0 }
    let alert = NSAlert()
    alert.alertStyle = .warning
    alert.messageText = "Authorize real test broadcast?"
    alert.informativeText = detail
    let cancel = alert.addButton(withTitle: "Cancel")
    let approve = alert.addButton(withTitle: "Authorize test broadcast")
    cancel.keyEquivalent = "\r"
    approve.keyEquivalent = ""
    return alert.runModal() == .alertSecondButtonReturn ? 1 : 0
}
