import AppKit
import ApplicationServices
import Carbon.HIToolbox
import Darwin
import IOKit

/// La saisie sécurisée de macOS (mot de passe, « Secure Keyboard Entry » d'un
/// terminal) vaut pour toute la session : `IsSecureEventInputEnabled()`
/// répond vrai partout dès qu'une seule application la tient, même en
/// arrière-plan. Elle ne concerne Dysflow que si c'est l'application au
/// premier plan qui la tient, ou si le champ qui a le focus est un champ de
/// mot de passe.
enum SecureInput {
    /// La règle : `holderPath`, l'exécutable qui tient la saisie sécurisée ;
    /// `frontmostBundlePath`, le paquet de l'application au premier plan.
    /// Sans l'un ou l'autre, on reste prudent.
    static func applies(enabled: Bool, holderPath: String?, frontmostBundlePath: String?) -> Bool {
        guard enabled else { return false }
        guard let holderPath, let frontmostBundlePath else { return true }
        let bundle = frontmostBundlePath.hasSuffix("/") ? frontmostBundlePath : frontmostBundlePath + "/"
        return holderPath.hasPrefix(bundle)
    }

    /// Vrai si la saisie sécurisée touche ce que l'utilisateur vise.
    static func active() -> Bool {
        if focusedFieldIsSecure() { return true }
        let enabled = IsSecureEventInputEnabled()
        guard enabled else { return false }
        let holder = holderPID().flatMap(executablePath)
        let front = NSWorkspace.shared.frontmostApplication?.bundleURL?.resolvingSymlinksInPath().path
        let applies = applies(enabled: enabled, holderPath: holder, frontmostBundlePath: front)
        if !applies {
            DictationLog.log.info("saisie sécurisée tenue en arrière-plan par \(holder ?? "?", privacy: .public) : ignorée")
        }
        return applies
    }

    /// Le processus qui tient la saisie sécurisée, tel que la session le publie.
    static func holderPID() -> pid_t? {
        // Publié sur la racine du registre (« ioreg -d1 »).
        let root = IORegistryGetRootEntry(kIOMainPortDefault)
        guard root != 0 else { return nil }
        defer { IOObjectRelease(root) }
        guard let users = IORegistryEntryCreateCFProperty(root, "IOConsoleUsers" as CFString,
                                                          kCFAllocatorDefault, 0)?
                .takeRetainedValue() as? [[String: Any]]
        else { return nil }
        let uid = Int(getuid())
        let mine = users.first { ($0["kCGSSessionUserIDKey"] as? Int) == uid }
        guard let pid = mine?["kCGSSessionSecureInputPID"] as? Int, pid > 0 else { return nil }
        return pid_t(pid)
    }

    private static func executablePath(_ pid: pid_t) -> String? {
        var buffer = [CChar](repeating: 0, count: Int(MAXPATHLEN) * 4)
        guard proc_pidpath(pid, &buffer, UInt32(buffer.count)) > 0 else { return nil }
        return URL(fileURLWithPath: String(cString: buffer)).resolvingSymlinksInPath().path
    }

    /// Le champ qui a le focus est un champ de mot de passe.
    private static func focusedFieldIsSecure() -> Bool {
        let system = AXUIElementCreateSystemWide()
        var focused: CFTypeRef?
        guard AXUIElementCopyAttributeValue(system, kAXFocusedUIElementAttribute as CFString,
                                            &focused) == .success,
              let element = focused, CFGetTypeID(element) == AXUIElementGetTypeID()
        else { return false }
        var subrole: CFTypeRef?
        return AXUIElementCopyAttributeValue(element as! AXUIElement, kAXSubroleAttribute as CFString,
                                             &subrole) == .success
            && (subrole as? String) == (kAXSecureTextFieldSubrole as String)
    }
}
