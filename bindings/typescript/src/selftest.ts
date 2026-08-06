import {
  decodeToJson,
  encodeJson,
  getPathJson,
  protectJson,
  schemaExportJson,
  validate,
  version,
} from "./index.js";

const data = encodeJson(
  JSON.stringify({
    server: { host: "localhost" },
    database: { password: "secret" },
  })
);

if (!validate(data)) {
  throw new Error("validate failed");
}

const host = getPathJson(data, "server.host");
if (host !== '"localhost"') {
  throw new Error(`unexpected host: ${host}`);
}

const schema = schemaExportJson(data);
if (schema.includes("secret")) {
  throw new Error(`agent-safe schema leaked value: ${schema}`);
}
if (!schema.includes("database") && !schema.includes("password")) {
  throw new Error(`expected schema paths, got: ${schema}`);
}

const protectedBytes = protectJson(
  JSON.stringify({ database: { password: "secret" } }),
  ["database.password"],
  "master"
);
const masked = decodeToJson(protectedBytes);
if (!masked.includes("[PROTECTED]")) {
  throw new Error(`expected masked output, got: ${masked}`);
}
const revealed = decodeToJson(protectedBytes, "master");
if (!revealed.includes("secret")) {
  throw new Error(`expected revealed password, got: ${revealed}`);
}

console.log(`bcs typescript bindings ok (version=${version()})`);
