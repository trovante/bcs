using Bcs;

var data = BcsClient.EncodeJson("""{"server":{"host":"localhost"},"database":{"password":"secret"}}""");
if (!BcsClient.Validate(data))
{
    throw new Exception("validate failed");
}

var host = BcsClient.GetPathJson(data, "server.host");
if (host != "\"localhost\"")
{
    throw new Exception($"unexpected host: {host}");
}

var schema = BcsClient.SchemaExportJson(data);
if (schema.Contains("secret"))
{
    throw new Exception($"agent-safe schema leaked value: {schema}");
}
if (!schema.Contains("database") && !schema.Contains("password"))
{
    throw new Exception($"expected schema paths, got: {schema}");
}

var protectedBytes = BcsClient.ProtectJson(
    """{"database":{"password":"secret"}}""",
    ["database.password"],
    "master");

var masked = BcsClient.DecodeToJson(protectedBytes);
if (!masked.Contains("[PROTECTED]"))
{
    throw new Exception($"expected masked output, got: {masked}");
}

var revealed = BcsClient.DecodeToJson(protectedBytes, "master");
if (!revealed.Contains("secret"))
{
    throw new Exception($"expected revealed password, got: {revealed}");
}

var secretRef = BcsClient.EncodeJson("""{"token":"__bcs_secret_ref__:env:API_TOKEN"}""");
var maskedRef = BcsClient.DecodeToJsonEx(secretRef);
if (!maskedRef.Contains("[SECRET_REF]"))
{
    throw new Exception($"expected masked secret reference, got: {maskedRef}");
}

var resolvedRef = BcsClient.DecodeToJsonEx(
    secretRef,
    resolveSecrets: (scheme, locator) =>
        scheme == "env" && locator == "API_TOKEN" ? "csharp-token" : null);
if (!resolvedRef.Contains("csharp-token"))
{
    throw new Exception($"expected resolved secret reference, got: {resolvedRef}");
}

Console.WriteLine($"bcs csharp bindings ok (version={BcsClient.Version()})");
