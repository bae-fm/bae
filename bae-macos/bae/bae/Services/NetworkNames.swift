import SystemConfiguration

/// The names System Settings gives the machine's network interfaces: the name
/// of the network service on an interface (Wi-Fi, a VPN), else the
/// interface's own localized name, else its BSD name.
struct NetworkNames: Sendable {
    private let byInterface: [String: String]

    init(byInterface: [String: String] = [:]) {
        self.byInterface = byInterface
    }

    /// The names as the system has them now.
    static func current() -> NetworkNames {
        var names: [String: String] = [:]
        for interface in SCNetworkInterfaceCopyAll() as? [SCNetworkInterface]
            ?? []
        {
            if let bsd = SCNetworkInterfaceGetBSDName(interface) as String?,
                let name = SCNetworkInterfaceGetLocalizedDisplayName(interface)
                    as String?
            {
                names[bsd] = name
            }
        }
        if let preferences = SCPreferencesCreate(nil, "bae" as CFString, nil),
            let services = SCNetworkServiceCopyAll(preferences)
                as? [SCNetworkService]
        {
            for service in services {
                if let interface = SCNetworkServiceGetInterface(service),
                    let bsd = SCNetworkInterfaceGetBSDName(interface)
                        as String?,
                    let name = SCNetworkServiceGetName(service) as String?
                {
                    names[bsd] = name
                }
            }
        }
        return NetworkNames(byInterface: names)
    }

    /// What to call the interface named `bsdName`.
    func name(of bsdName: String) -> String {
        byInterface[bsdName] ?? bsdName
    }
}
