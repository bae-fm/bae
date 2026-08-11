import BaeKit
import Foundation
import Network
import OSLog

private let logger = Logger.bae("RendererBrowser")

/// Finds cast receivers with the system's Bonjour browser and reports each one
/// to core, which decides what the service means.
///
/// iOS is the one platform where bae does not read the network itself: an app
/// socket may not join a multicast group without an entitlement Apple grants by
/// application, while the system browser needs only the service list declared in
/// `Info.plist`. So core names the service types to browse, this resolves each
/// result to an address, and core maps the TXT record to a device exactly as its
/// own mDNS browse would. Runs with the picker, like discovery everywhere else.
@MainActor
final class RendererBrowser: Observable {
    /// Identity of one advertised service: the tag core reports it under, plus
    /// the instance name the browser named it by.
    private struct ServiceKey: Hashable {
        let serviceType: BridgeRendererServiceType
        let instanceName: String
    }

    private let services: [BridgeRendererService]
    private let found: @Sendable (BridgeReportedRenderer) -> Void
    private let lost: @Sendable (BridgeRendererServiceType, String) -> Void

    private var browsers: [NWBrowser] = []
    /// The connections opened purely to learn a service's address, keyed so a
    /// re-advertised service replaces its own in-flight resolve.
    private var resolving: [ServiceKey: NWConnection] = [:]

    init(
        services: [BridgeRendererService],
        found: @escaping @Sendable (BridgeReportedRenderer) -> Void,
        lost: @escaping @Sendable (BridgeRendererServiceType, String) -> Void
    ) {
        self.services = services
        self.found = found
        self.lost = lost
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            services: handle.getRendererServiceTypes(),
            found: { handle.rendererFound(service: $0) },
            lost: { handle.rendererLost(serviceType: $0, instanceName: $1) }
        )
    }

    #if DEBUG
    // periphery:ignore
    static func stub() -> RendererBrowser {
        RendererBrowser(
            services: [],
            found: { _ in },
            lost: { _, _ in }
        )
    }
    #endif

    /// Begin browsing every service type core names. Idempotent.
    func start() {
        guard browsers.isEmpty else {
            return
        }
        for service in services {
            let browser = NWBrowser(
                for: .bonjourWithTXTRecord(
                    type: service.dnsSdType,
                    domain: nil
                ),
                using: .tcp
            )
            browser.browseResultsChangedHandler = { _, changes in
                MainActor.assumeIsolated {
                    self.apply(changes, serviceType: service.serviceType)
                }
            }
            browser.stateUpdateHandler = { state in
                if case .failed(let error) = state {
                    logger.error(
                        "Browsing \(service.dnsSdType) failed: \(error.localizedDescription)"
                    )
                }
            }
            browser.start(queue: .main)
            browsers.append(browser)
        }
    }

    /// Stop browsing and drop every in-flight resolve. Core keeps the last list
    /// it published until the next browse starts, the same as when bae browses
    /// for itself.
    func stop() {
        for browser in browsers {
            browser.cancel()
        }
        browsers.removeAll()
        for connection in resolving.values {
            connection.cancel()
        }
        resolving.removeAll()
    }

    private func apply(
        _ changes: Set<NWBrowser.Result.Change>,
        serviceType: BridgeRendererServiceType
    ) {
        for change in changes {
            switch change {
            case .added(let result):
                resolve(result, serviceType: serviceType)

            // A changed TXT record or interface set re-reports the service, so
            // core sees the current record rather than the one it first saw.
            case .changed(_, let new, _):
                resolve(new, serviceType: serviceType)

            case .removed(let result):
                guard let name = Self.instanceName(of: result) else {
                    continue
                }
                resolving.removeValue(
                    forKey: ServiceKey(
                        serviceType: serviceType,
                        instanceName: name
                    )
                )?
                .cancel()
                lost(serviceType, name)

            case .identical:
                break

            @unknown default:
                logger.debug("Ignoring an unknown browse change")
            }
        }
    }

    /// A Bonjour result names a service, not an address. Connecting to it is how
    /// the Network framework resolves one; the connection is cancelled as soon as
    /// the path reports where it landed.
    private func resolve(
        _ result: NWBrowser.Result,
        serviceType: BridgeRendererServiceType
    ) {
        guard let name = Self.instanceName(of: result) else {
            return
        }
        let key = ServiceKey(serviceType: serviceType, instanceName: name)
        resolving[key]?.cancel()

        let txt = Self.txtRecord(of: result)
        let connection = NWConnection(to: result.endpoint, using: .tcp)
        connection.stateUpdateHandler = { state in
            MainActor.assumeIsolated {
                switch state {
                case .ready:
                    self.report(
                        connection: connection,
                        key: key,
                        txt: txt
                    )
                    self.finishResolving(key)

                case .failed(let error):
                    logger.debug(
                        "Could not reach \(name): \(error.localizedDescription)"
                    )
                    self.finishResolving(key)

                case .cancelled:
                    self.finishResolving(key)

                default:
                    break
                }
            }
        }
        resolving[key] = connection
        connection.start(queue: .main)
    }

    private func report(
        connection: NWConnection,
        key: ServiceKey,
        txt: [String: String]
    ) {
        guard
            case .hostPort(let host, let port)? = connection.currentPath?
                .remoteEndpoint,
            let address = Self.address(of: host)
        else {
            logger.debug(
                "Resolved \(key.instanceName) to no usable address"
            )
            return
        }
        found(
            BridgeReportedRenderer(
                serviceType: key.serviceType,
                instanceName: key.instanceName,
                addr: address,
                port: port.rawValue,
                txt: txt
            )
        )
    }

    private func finishResolving(_ key: ServiceKey) {
        guard let connection = resolving.removeValue(forKey: key) else {
            return
        }
        connection.cancel()
    }

    /// The instance name a Bonjour result advertises — the identity core keys a
    /// reported service by, and the one a later removal names.
    private static func instanceName(of result: NWBrowser.Result) -> String? {
        guard case .service(let name, _, _, _) = result.endpoint else {
            logger.debug("Ignoring a browse result that names no service")
            return nil
        }
        return name
    }

    private static func txtRecord(
        of result: NWBrowser.Result
    ) -> [String: String] {
        guard case .bonjour(let record) = result.metadata else {
            return [:]
        }
        return record.dictionary
    }

    /// The address text core parses. An IPv6 address carries the interface it
    /// was learned on (`fe80::1%en0`), which is not part of the address itself.
    private static func address(of host: NWEndpoint.Host) -> String? {
        switch host {
        case .ipv4(let address):
            return address.debugDescription

        case .ipv6(let address):
            return address.debugDescription.split(separator: "%").first
                .map(String.init)

        case .name(let name, _):
            logger.debug("Resolved to the name \(name), not an address")
            return nil

        @unknown default:
            return nil
        }
    }
}
