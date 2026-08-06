# Agent-safe schema JSON

Emitted by `bcs schema --agent-safe` and FFI `bcs_schema_export_json`.

## Shape

```json
{
  "version": "1.0",
  "root": "Root",
  "sensitive_paths": ["database.password", "api.key"],
  "paths": [
    {
      "path": "database.host",
      "type_name": "string",
      "required": true,
      "documentation": "DB hostname",
      "constraints": [],
      "sensitive": false
    },
    {
      "path": "database.password",
      "type_name": "string",
      "required": true,
      "sensitive": true
    }
  ]
}
```

## Guarantees

- No field **values** from the data layer.
- Defaults that look like secrets are omitted from this view (export never includes defaults).
- Protect markers / secret refs are never expanded to plaintext here (schema-only).

## CLI

```bash
bcs schema --agent-safe config.bcs
bcs schema --agent-safe -e out.json config.bcs
```
