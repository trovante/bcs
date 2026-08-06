import Bcs
import Foundation

@main
struct BcsSelfTest {
    static func main() throws {
        let data = try Bcs.encodeJson(
            #"{"server":{"host":"localhost"},"database":{"password":"secret"}}"#
        )
        let valid = try Bcs.validate(data)
        guard valid else {
            throw BcsError(code: -1, message: "validate failed")
        }

        let host = try Bcs.getPathJson(data, path: "server.host")
        guard host == #""localhost""# else {
            throw BcsError(code: -1, message: "unexpected host: \(host)")
        }

        let schema = try Bcs.schemaExportJson(data)
        guard !schema.contains("secret") else {
            throw BcsError(code: -1, message: "agent-safe schema leaked value: \(schema)")
        }
        guard schema.contains("database") || schema.contains("password") else {
            throw BcsError(code: -1, message: "expected schema paths, got: \(schema)")
        }

        let protected = try Bcs.protectJson(
            #"{"database":{"password":"secret"}}"#,
            paths: ["database.password"],
            password: "master"
        )
        let masked = try Bcs.decodeToJson(protected)
        guard masked.contains("[PROTECTED]") else {
            throw BcsError(code: -1, message: "expected masked output")
        }

        let revealed = try Bcs.decodeToJson(protected, password: "master")
        guard revealed.contains("secret") else {
            throw BcsError(code: -1, message: "expected revealed password")
        }

        let secretRef = try Bcs.encodeJson(
            #"{"token":"__bcs_secret_ref__:env:API_TOKEN"}"#
        )
        let maskedRef = try Bcs.decodeToJsonEx(secretRef)
        guard maskedRef.contains("[SECRET_REF]") else {
            throw BcsError(code: -1, message: "expected masked secret reference")
        }
        let resolvedRef = try Bcs.decodeToJsonEx(
            secretRef,
            resolveSecrets: { scheme, locator in
                scheme == "env" && locator == "API_TOKEN" ? "swift-token" : nil
            }
        )
        guard resolvedRef.contains("swift-token") else {
            throw BcsError(code: -1, message: "expected resolved secret reference")
        }

        let protectedEx = try Bcs.protectJsonEx(
            #"{"database":{"password":"secret-ex"}}"#,
            paths: ["database.password"],
            password: "master-ex"
        )
        let revealedEx = try Bcs.decodeToJson(protectedEx, password: "master-ex")
        guard revealedEx.contains("secret-ex") else {
            throw BcsError(code: -1, message: "expected protectJsonEx password round trip")
        }

        print("bcs swift bindings ok (version=\(Bcs.version()))")
    }
}
