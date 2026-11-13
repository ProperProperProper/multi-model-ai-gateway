import Foundation
import FoundationModels

@available(macOS 26.0, *)
struct Request: Decodable {
    let prompt: String?
    let mode: String?
    let instructions: String?
}

@available(macOS 26.0, *)
struct Reply: Encodable {
    let ok: Bool
    let provider: String
    let text: String?
    let error: String?
}

@available(macOS 26.0, *)
struct StatusReply: Encodable {
    let ok: Bool
    let onDevice: String
    let privateCloud: String
}

@available(macOS 26.0, *)
enum BridgeError: LocalizedError {
    case missingPrompt
    case unavailable(String)

    var errorDescription: String? {
        switch self {
        case .missingPrompt: return "prompt is required"
        case .unavailable(let reason): return reason
        }
    }
}

@available(macOS 26.0, *)
enum MultiModelAppleAI {
    static let encoder = JSONEncoder()

    static func write<T: Encodable>(_ value: T) {
        if let data = try? encoder.encode(value), let line = String(data: data, encoding: .utf8) {
            FileHandle.standardOutput.write(Data((line + "\n").utf8))
        }
    }

    static func availability(_ value: SystemLanguageModel.Availability) -> String {
        switch value {
        case .available: return "available"
        case .unavailable(let reason): return "unavailable:\(String(describing: reason))"
        }
    }

    static func status() {
        let local = SystemLanguageModel.default
        write(StatusReply(ok: true, onDevice: availability(local.availability), privateCloud: "unavailable: not exposed by the installed Foundation Models SDK"))
    }

    static func respond(_ request: Request) async {
        do {
            guard let prompt = request.prompt, !prompt.isEmpty else { throw BridgeError.missingPrompt }
            let mode = request.mode ?? "on_device"
            let instructions = request.instructions ?? "You are Multi-Model AI Gateway's Apple-native assistant. Be concise and return useful plain text."
            // The public macOS 26 SDK does not expose Private Cloud Compute.
            // Keep private_cloud requests compatible by falling back locally.
            let model = SystemLanguageModel.default
            guard model.availability == .available else { throw BridgeError.unavailable("Apple on-device Foundation Model is unavailable: \(availability(model.availability))") }
            let session = LanguageModelSession(model: model, instructions: instructions)
            let text = try await session.respond(to: prompt).content
            write(Reply(ok: true, provider: mode == "private_cloud" ? "apple-foundation-model-fallback" : "apple-foundation-model", text: text, error: nil))
        } catch {
            write(Reply(ok: false, provider: "apple", text: nil, error: error.localizedDescription))
        }
    }

    static func run() async {
        if CommandLine.arguments.contains("--status") { status(); return }
        while let line = readLine() {
            guard let data = line.data(using: .utf8), let request = try? JSONDecoder().decode(Request.self, from: data) else {
                write(Reply(ok: false, provider: "apple", text: nil, error: "invalid JSON request")); continue
            }
            await respond(request)
        }
    }
}

@main
@available(macOS 26.0, *)
struct Main {
    static func main() async { await MultiModelAppleAI.run() }
}

