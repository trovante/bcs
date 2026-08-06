import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import koffi, { type IKoffiRegisteredCallback } from "koffi";

export const BCS_OK = 0;

export class BCSError extends Error {
  readonly code: number;

  constructor(code: number, message: string) {
    super(`BCS error ${code}: ${message}`);
    this.code = code;
    this.name = "BCSError";
  }
}

type Native = {
  version: () => string | null;
  lastError: () => string | null;
  encodeJson: (
    json: string,
    compact: number,
    compressData: number,
    outPtr: Buffer,
    outLen: Buffer
  ) => number;
  decodeToJson: (
    data: Buffer,
    len: bigint,
    password: string | null,
    outJson: Buffer
  ) => number;
  decodeToJsonEx: (
    data: Buffer,
    len: bigint,
    password: string | null,
    resolveFn: unknown,
    resolveUserdata: unknown,
    unwrapFn: unknown,
    unwrapUserdata: unknown,
    outJson: Buffer
  ) => number;
  getPathJson: (
    data: Buffer,
    len: bigint,
    pathQuery: string,
    outJson: Buffer
  ) => number;
  schemaExportJson: (data: Buffer, len: bigint, outJson: Buffer) => number;
  validate: (data: Buffer, len: bigint, outOk: Buffer) => number;
  protectJson: (
    json: string,
    pathsCsv: string,
    password: string,
    compact: number,
    compressData: number,
    outPtr: Buffer,
    outLen: Buffer
  ) => number;
  protectJsonEx: (
    json: string,
    pathsCsv: string,
    password: string | null,
    kmsProvider: string | null,
    kmsKey: string | null,
    wrapFn: unknown,
    wrapUserdata: unknown,
    compact: number,
    compressData: number,
    outPtr: Buffer,
    outLen: Buffer
  ) => number;
  strdup: (s: string) => unknown;
  alloc: (len: bigint) => unknown;
  freeBuffer: (ptr: unknown, len: bigint) => void;
  freeString: (ptr: unknown) => void;
};

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ptrSize = koffi.sizeof("void *");
const sizeSize = koffi.sizeof("size_t");

function repoRoot(): string {
  return path.resolve(__dirname, "../../..");
}

function candidateLibs(): string[] {
  const root = repoRoot();
  const system = os.platform();
  const arch = os.arch() === "arm64" ? "arm64" : "x64";
  const osName =
    system === "darwin" ? "darwin" : system === "linux" ? "linux" : system;

  const names =
    system === "darwin"
      ? ["libbcs_ffi.dylib"]
      : system === "win32"
        ? ["bcs_ffi.dll", "libbcs_ffi.dll"]
        : ["libbcs_ffi.so"];

  const bases = [
    process.env.BCS_FFI_LIB ? path.dirname(process.env.BCS_FFI_LIB) : null,
    path.join(root, "dist", "ffi", `${osName}-${arch}`),
    path.join(root, "target", "release"),
    path.join(root, "target", "debug"),
    process.env.CARGO_TARGET_DIR
      ? path.join(process.env.CARGO_TARGET_DIR, "release")
      : null,
    process.env.CARGO_TARGET_DIR
      ? path.join(process.env.CARGO_TARGET_DIR, "debug")
      : null,
  ].filter(Boolean) as string[];

  const out: string[] = [];
  if (process.env.BCS_FFI_LIB) out.push(process.env.BCS_FFI_LIB);
  for (const base of bases) {
    for (const name of names) out.push(path.join(base, name));
  }
  return out;
}

let cached: Native | null = null;

function loadNative(): Native {
  if (cached) return cached;

  const found = candidateLibs().find((p) => fs.existsSync(p));
  if (!found) {
    throw new Error(
      "Could not load bcs_ffi. Build with `cargo build -p bcs-ffi --release` or set BCS_FFI_LIB."
    );
  }

  const lib = koffi.load(found);
  cached = {
    version: lib.func("bcs_version", "str", []),
    lastError: lib.func("bcs_last_error", "str", []),
    encodeJson: lib.func("bcs_encode_json", "int", [
      "str",
      "int",
      "int",
      "void *",
      "void *",
    ]),
    decodeToJson: lib.func("bcs_decode_to_json", "int", [
      "void *",
      "uint64",
      "str",
      "void *",
    ]),
    getPathJson: lib.func("bcs_get_path_json", "int", [
      "void *",
      "uint64",
      "str",
      "void *",
    ]),
    schemaExportJson: lib.func("bcs_schema_export_json", "int", [
      "void *",
      "uint64",
      "void *",
    ]),
    validate: lib.func("bcs_validate", "int", ["void *", "uint64", "void *"]),
    protectJson: lib.func("bcs_protect_json", "int", [
      "str",
      "str",
      "str",
      "int",
      "int",
      "void *",
      "void *",
    ]),
    freeBuffer: lib.func("bcs_free_buffer", "void", ["void *", "uint64"]),
    freeString: lib.func("bcs_free_string", "void", ["void *"]),
    strdup: lib.func("bcs_strdup", "void *", ["str"]),
    alloc: lib.func("bcs_alloc", "void *", ["uint64"]),
    decodeToJsonEx: lib.func("bcs_decode_to_json_ex", "int", [
      "void *",
      "uint64",
      "str",
      "void *",
      "void *",
      "void *",
      "void *",
      "void *",
    ]),
    protectJsonEx: lib.func("bcs_protect_json_ex", "int", [
      "str",
      "str",
      "str",
      "str",
      "str",
      "void *",
      "void *",
      "int",
      "int",
      "void *",
      "void *",
    ]),
  };

  return cached;
}

function check(code: number): void {
  if (code !== BCS_OK) {
    throw new BCSError(code, loadNative().lastError() ?? "unknown error");
  }
}

function readPtr(buf: Buffer): unknown {
  return koffi.decode(buf, "void *");
}

function readSize(buf: Buffer): bigint {
  return sizeSize === 8 ? buf.readBigUInt64LE(0) : BigInt(buf.readUInt32LE(0));
}

/** Deep-copy bytes from a native pointer before freeing it. */
function takeBytes(ptr: unknown, len: bigint): Buffer {
  const tmp = Buffer.from(koffi.view(ptr, Number(len)));
  const copy = Buffer.alloc(tmp.length);
  tmp.copy(copy);
  return copy;
}

/** Read a NUL-terminated UTF-8 C string from a native pointer. */
function takeString(ptr: unknown): string {
  const tmp = Buffer.from(koffi.view(ptr, 1024 * 1024));
  const end = tmp.indexOf(0);
  return tmp.slice(0, end < 0 ? tmp.length : end).toString("utf8");
}

export function version(): string {
  return loadNative().version() ?? "";
}

export function encodeJson(
  jsonText: string,
  opts: { compact?: boolean; compressData?: boolean } = {}
): Buffer {
  const native = loadNative();
  const outPtr = Buffer.alloc(ptrSize);
  const outLen = Buffer.alloc(sizeSize);
  const code = native.encodeJson(
    jsonText,
    opts.compact ? 1 : 0,
    opts.compressData ? 1 : 0,
    outPtr,
    outLen
  );
  check(code);
  const ptr = readPtr(outPtr);
  const len = readSize(outLen);
  try {
    return takeBytes(ptr, len);
  } finally {
    native.freeBuffer(ptr, len);
  }
}

export function decodeToJson(data: Buffer, password?: string): string {
  const native = loadNative();
  const outJson = Buffer.alloc(ptrSize);
  const code = native.decodeToJson(
    data,
    BigInt(data.length),
    password ?? null,
    outJson
  );
  check(code);
  const ptr = readPtr(outJson);
  try {
    return takeString(ptr);
  } finally {
    native.freeString(ptr);
  }
}

export function getPathJson(data: Buffer, pathQuery: string): string {
  const native = loadNative();
  const outJson = Buffer.alloc(ptrSize);
  const code = native.getPathJson(
    data,
    BigInt(data.length),
    pathQuery,
    outJson
  );
  check(code);
  const ptr = readPtr(outJson);
  try {
    return takeString(ptr);
  } finally {
    native.freeString(ptr);
  }
}

/** Agent-safe schema JSON (paths/types/sensitive; never data-layer values). */
export function schemaExportJson(data: Buffer): string {
  const native = loadNative();
  const outJson = Buffer.alloc(ptrSize);
  const code = native.schemaExportJson(data, BigInt(data.length), outJson);
  check(code);
  const ptr = readPtr(outJson);
  try {
    return takeString(ptr);
  } finally {
    native.freeString(ptr);
  }
}

export function validate(data: Buffer): boolean {
  const native = loadNative();
  const outOk = Buffer.alloc(4);
  const code = native.validate(data, BigInt(data.length), outOk);
  check(code);
  return outOk.readInt32LE(0) === 1;
}

export function protectJson(
  jsonText: string,
  paths: string[],
  password: string,
  opts: { compact?: boolean; compressData?: boolean } = {}
): Buffer {
  const native = loadNative();
  const outPtr = Buffer.alloc(ptrSize);
  const outLen = Buffer.alloc(sizeSize);
  const code = native.protectJson(
    jsonText,
    paths.join(","),
    password,
    opts.compact ? 1 : 0,
    opts.compressData ? 1 : 0,
    outPtr,
    outLen
  );
  check(code);
  const ptr = readPtr(outPtr);
  const len = readSize(outLen);
  try {
    return takeBytes(ptr, len);
  } finally {
    native.freeBuffer(ptr, len);
  }
}

export type SecretResolveFn = (scheme: string, locator: string) => string | null;
export type KeyUnwrapFn = (
  provider: string,
  kekLocator: string,
  wrapped: Uint8Array
) => Uint8Array | null;

export function decodeToJsonEx(
  data: Buffer,
  opts: {
    password?: string;
    resolveSecrets?: SecretResolveFn;
    unwrapKey?: KeyUnwrapFn;
  } = {}
): string {
  const native = loadNative();
  const outJson = Buffer.alloc(ptrSize);

  let resolveCb: IKoffiRegisteredCallback | null = null;
  let unwrapCb: IKoffiRegisteredCallback | null = null;

  if (opts.resolveSecrets) {
    const userFn = opts.resolveSecrets;
    resolveCb = koffi.register(
      (scheme: string, locator: string, _ud: unknown) => {
        const result = userFn(scheme, locator);
        if (result === null) return null;
        return native.strdup(result);
      },
      "void *(str, str, void *)"
    );
  }

  if (opts.unwrapKey) {
    const userFn = opts.unwrapKey;
    unwrapCb = koffi.register(
      (
        provider: string,
        kekLocator: string,
        wrappedPtr: unknown,
        wrappedLen: bigint,
        outDek: unknown,
        _ud: unknown
      ) => {
        const wrapped = Buffer.from(
          koffi.view(koffi.decode(wrappedPtr as Buffer, "void *"), Number(wrappedLen))
        );
        const result = userFn(provider, kekLocator, wrapped);
        if (result === null) return 1;
        if (result.length !== 32) return 1;
        const dekView = Buffer.from(
          koffi.decode(outDek as Buffer, "void *"),
          0,
          32
        );
        Buffer.from(result).copy(dekView);
        return 0;
      },
      "int(str, str, void *, uint64, void *, void *)"
    );
  }

  try {
    const code = native.decodeToJsonEx(
      data,
      BigInt(data.length),
      opts.password ?? null,
      resolveCb,
      null,
      unwrapCb,
      null,
      outJson
    );
    check(code);
    const ptr = readPtr(outJson);
    try {
      return takeString(ptr);
    } finally {
      native.freeString(ptr);
    }
  } finally {
    if (resolveCb) koffi.unregister(resolveCb);
    if (unwrapCb) koffi.unregister(unwrapCb);
  }
}

export type KeyWrapFn = (
  provider: string,
  kekLocator: string,
  dek: Uint8Array
) => Uint8Array | null;

export function protectJsonEx(
  jsonText: string,
  paths: string[],
  opts: {
    password?: string;
    kmsProvider?: string;
    kmsKey?: string;
    wrapKey?: KeyWrapFn;
    compact?: boolean;
    compressData?: boolean;
  } = {}
): Buffer {
  const native = loadNative();
  const outPtr = Buffer.alloc(ptrSize);
  const outLen = Buffer.alloc(sizeSize);

  let wrapCb: IKoffiRegisteredCallback | null = null;

  if (opts.wrapKey && opts.kmsProvider && opts.kmsKey) {
    const userFn = opts.wrapKey;
    wrapCb = koffi.register(
      (
        provider: string,
        kekLocator: string,
        dekPtr: unknown,
        dekLen: bigint,
        outWrappedPtr: unknown,
        outWrappedLenPtr: unknown,
        _ud: unknown
      ) => {
        const dek = Buffer.from(
          koffi.view(koffi.decode(dekPtr as Buffer, "void *"), Number(dekLen))
        );
        const result = userFn(provider, kekLocator, dek);
        if (result === null) return 1;
        const buf = native.alloc(BigInt(result.length));
        Buffer.from(result).copy(
          Buffer.from(koffi.view(buf, result.length))
        );
        koffi.encode(outWrappedPtr as Buffer, "void *", buf);
        koffi.encode(outWrappedLenPtr as Buffer, "uint64", BigInt(result.length));
        return 0;
      },
      "int(str, str, void *, uint64, void *, void *, void *)"
    );
  }

  try {
    const code = native.protectJsonEx(
      jsonText,
      paths.join(","),
      opts.password ?? null,
      opts.kmsProvider ?? null,
      opts.kmsKey ?? null,
      wrapCb,
      null,
      opts.compact ? 1 : 0,
      opts.compressData ? 1 : 0,
      outPtr,
      outLen
    );
    check(code);
    const ptr = readPtr(outPtr);
    const len = readSize(outLen);
    try {
      return takeBytes(ptr, len);
    } finally {
      native.freeBuffer(ptr, len);
    }
  } finally {
    if (wrapCb) koffi.unregister(wrapCb);
  }
}
