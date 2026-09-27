import SwiftUI
import UIKit

/// Original Phosphor vectors used by the Tauri mobile dock, at its fixed 27pt size.
struct MacroIcon: View {
    let name: String
    var size: CGFloat = 27
    var body: some View { Image("macro-" + name).renderingMode(.template).resizable().scaledToFit().frame(width: size, height: size).accessibilityHidden(true) }
}

extension NativeTab {
    func dockIcon(selected: Bool) -> String {
        switch self {
        case .home: selected ? "bell-fill" : "bell"
        case .calendar: selected ? "calendar-fill" : "calendar"
        case .email: selected ? "envelope-fill" : "envelope"
        case .channels: selected ? "hash-straight-fill" : "hash-straight"
        case .files: selected ? "files-fill" : "files"
        case .agents: selected ? "sparkle-fill" : "sparkle"
        case .tasks: "list-checks"
        case .calls: "phone-call"
        }
    }
}

private struct NativeChromeTopKey: EnvironmentKey { static let defaultValue: CGFloat = 0 }
private struct NativeChromeBottomKey: EnvironmentKey { static let defaultValue: CGFloat = 0 }
extension EnvironmentValues {
    var nativeChromeTop: CGFloat {
        get { self[NativeChromeTopKey.self] }
        set { self[NativeChromeTopKey.self] = newValue }
    }
    var nativeChromeBottom: CGFloat {
        get { self[NativeChromeBottomKey.self] }
        set { self[NativeChromeBottomKey.self] = newValue }
    }
}

private struct NativeChromeInset: ViewModifier {
    @Environment(\.nativeChromeBottom) private var bottom
    func body(content: Content) -> some View {
        content.safeAreaInset(edge: .bottom, spacing: 0) { Color.clear.frame(height: bottom).allowsHitTesting(false).accessibilityHidden(true) }
    }
}
extension View { func nativeChromeInset() -> some View { modifier(NativeChromeInset()) } }

struct MacroDockButtonStyle: ButtonStyle {
    let tab: NativeTab
    let selected: Bool
    func makeBody(configuration: Configuration) -> some View {
        MacroIcon(name: tab.dockIcon(selected: selected || configuration.isPressed))
            .foregroundStyle(selected ? MacroTheme.accent : .primary)
            .frame(width: 46, height: 46).contentShape(Circle())
            .onChange(of: configuration.isPressed) { _, pressed in if pressed { UIImpactFeedbackGenerator(style: .light).impactOccurred() } }
    }
}

// UIImage's SVG natural size is256pt; explicitly rasterize at the chrome's point size.
extension MacroIcon {
    static func image(_ name: String, size: CGFloat) -> UIImage? {
        guard let source = UIImage(named: "macro-" + name) else { return nil }
        return UIGraphicsImageRenderer(size: CGSize(width: size, height: size)).image { _ in
            source.draw(in: CGRect(x: 0, y: 0, width: size, height: size))
        }.withRenderingMode(.alwaysTemplate)
    }
}
