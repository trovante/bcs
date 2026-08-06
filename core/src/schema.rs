// Schema engine implementation

use crate::error::{BCSError, Result};
use crate::types::Value;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::Read;

// ============================================================================
// Type Definitions
// ============================================================================

/// Type definition for schema types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeDefinition {
    // Primitive types
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    Bool,
    String,
    Bytes,
    Null,

    // Composite types
    List(Box<TypeDefinition>),
    Map(Box<TypeDefinition>, Box<TypeDefinition>), // (key_type, value_type)
    Struct(HashMap<String, FieldDefinition>),
    Union(Vec<VariantDefinition>),
    Optional(Box<TypeDefinition>),

    // Custom types
    Custom(String), // Reference to a custom type by name
}

/// Field definition for struct types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub field_type: TypeDefinition,
    pub required: bool,
    pub default: Option<Value>,
    pub constraints: Vec<Constraint>,
    pub documentation: Option<String>,
    pub ai_tag: Option<String>,
}

/// Variant definition for union types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariantDefinition {
    pub name: String,
    pub tag: u32,
    pub value_type: TypeDefinition,
    pub documentation: Option<String>,
}

// ============================================================================
// Constraints
// ============================================================================

/// Constraint for validation rules
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Constraint {
    /// Range constraint for numeric types (min, max)
    Range(f64, f64),

    /// Pattern constraint for strings (regex pattern)
    Pattern(String),

    /// Non-empty constraint for strings, lists, maps
    NonEmpty,

    /// Length constraint (min, max) for strings, lists, maps
    Length(Option<usize>, Option<usize>),

    /// Unique constraint for list elements
    Unique,

    /// Custom constraint with validation expression
    Custom(String),

    /// Enum constraint - value must be one of the specified values
    Enum(Vec<Value>),

    /// Minimum value constraint
    Min(f64),

    /// Maximum value constraint
    Max(f64),
}

// ============================================================================
// AI Semantic Tags
// ============================================================================

/// AI semantic tag for configuration fields
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AISemanticTag {
    pub category: String,
    pub subcategory: Option<String>,
    pub description: String,
    pub embedding: Option<Vec<f32>>, // Optional 384-dim vector
    pub sensitivity: Option<String>, // e.g., "high", "medium", "low"
}

impl AISemanticTag {
    /// Create a new AI semantic tag
    pub fn new(category: String, description: String) -> Self {
        Self {
            category,
            subcategory: None,
            description,
            embedding: None,
            sensitivity: None,
        }
    }

    /// Set subcategory
    pub fn with_subcategory(mut self, subcategory: String) -> Self {
        self.subcategory = Some(subcategory);
        self
    }

    /// Set embedding
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Set sensitivity level
    pub fn with_sensitivity(mut self, sensitivity: String) -> Self {
        self.sensitivity = Some(sensitivity);
        self
    }
}

// ============================================================================
// Schema
// ============================================================================

/// Schema definition for BCS files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    /// Version of the schema
    pub version: String,

    /// Type definitions (name -> type)
    pub types: HashMap<String, TypeDefinition>,

    /// Root type name
    pub root: String,

    /// Constraints mapped by path
    pub constraints: HashMap<String, Vec<Constraint>>,

    /// Documentation strings mapped by path
    pub documentation: HashMap<String, String>,

    /// AI semantic tags mapped by tag name
    pub ai_tags: HashMap<String, AISemanticTag>,

    /// Paths marked sensitive (source of truth for agent-safe / validate policy).
    /// Additive; older MessagePack schemas without this field deserialize as empty.
    #[serde(default)]
    pub sensitive_paths: HashMap<String, bool>,
}

impl Schema {
    /// Create a new empty schema
    pub fn new(root: String) -> Self {
        Self {
            version: "1.0".to_string(),
            types: HashMap::new(),
            root,
            constraints: HashMap::new(),
            documentation: HashMap::new(),
            ai_tags: HashMap::new(),
            sensitive_paths: HashMap::new(),
        }
    }

    /// Add a type definition
    pub fn add_type(&mut self, name: String, type_def: TypeDefinition) {
        self.types.insert(name, type_def);
    }

    /// Get a type definition by name
    pub fn get_type(&self, name: &str) -> Option<&TypeDefinition> {
        self.types.get(name)
    }

    /// Add constraints for a path
    pub fn add_constraints(&mut self, path: String, constraints: Vec<Constraint>) {
        self.constraints.insert(path, constraints);
    }

    /// Get constraints for a path
    pub fn get_constraints(&self, path: &str) -> Option<&Vec<Constraint>> {
        self.constraints.get(path)
    }

    /// Add documentation for a path
    pub fn add_documentation(&mut self, path: String, doc: String) {
        self.documentation.insert(path, doc);
    }

    /// Get documentation for a path
    pub fn get_documentation(&self, path: &str) -> Option<&String> {
        self.documentation.get(path)
    }

    /// Add an AI semantic tag
    pub fn add_ai_tag(&mut self, name: String, tag: AISemanticTag) {
        self.ai_tags.insert(name, tag);
    }

    /// Get an AI semantic tag by name
    pub fn get_ai_tag(&self, name: &str) -> Option<&AISemanticTag> {
        self.ai_tags.get(name)
    }

    /// Mark a dotted path as sensitive (does not encrypt data).
    pub fn mark_sensitive(&mut self, path: impl Into<String>) {
        self.sensitive_paths.insert(path.into(), true);
    }

    /// Mark multiple dotted paths as sensitive.
    pub fn mark_sensitive_paths<I, S>(&mut self, paths: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for path in paths {
            self.mark_sensitive(path);
        }
    }

    /// Return true when the path is marked sensitive.
    pub fn is_sensitive(&self, path: &str) -> bool {
        self.sensitive_paths.get(path).copied().unwrap_or(false)
    }

    /// Sorted list of paths marked sensitive.
    pub fn sensitive_path_list(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .sensitive_paths
            .iter()
            .filter(|(_, flagged)| **flagged)
            .map(|(path, _)| path.clone())
            .collect();
        paths.sort();
        paths
    }

    /// Get the root type definition
    pub fn get_root_type(&self) -> Result<&TypeDefinition> {
        self.get_type(&self.root)
            .ok_or_else(|| BCSError::Validation(format!("Root type '{}' not found", self.root)))
    }

    /// Parse schema from MessagePack-encoded bytes
    pub fn from_msgpack(data: &[u8]) -> Result<Self> {
        rmp_serde::from_slice(data).map_err(|e| {
            BCSError::Decoding(format!("Failed to parse schema from MessagePack: {}", e))
        })
    }

    /// Parse schema from MessagePack-encoded reader
    pub fn from_msgpack_reader<R: Read>(reader: R) -> Result<Self> {
        rmp_serde::from_read(reader).map_err(|e| {
            BCSError::Decoding(format!("Failed to parse schema from MessagePack: {}", e))
        })
    }

    /// Encode schema to MessagePack bytes
    pub fn to_msgpack(&self) -> Result<Vec<u8>> {
        let canonical = CanonicalSchema::from_schema(self);
        rmp_serde::to_vec(&canonical).map_err(|e| {
            BCSError::Encoding(format!("Failed to encode schema to MessagePack: {}", e))
        })
    }
}

#[derive(Debug, Clone, Serialize)]
struct CanonicalSchema {
    version: String,
    types: BTreeMap<String, CanonicalTypeDefinition>,
    root: String,
    constraints: BTreeMap<String, Vec<Constraint>>,
    documentation: BTreeMap<String, String>,
    ai_tags: BTreeMap<String, AISemanticTag>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    sensitive_paths: BTreeMap<String, bool>,
}

impl CanonicalSchema {
    fn from_schema(schema: &Schema) -> Self {
        let mut types = BTreeMap::new();
        for (name, type_def) in &schema.types {
            types.insert(name.clone(), CanonicalTypeDefinition::from_type(type_def));
        }

        let mut constraints = BTreeMap::new();
        for (path, rules) in &schema.constraints {
            constraints.insert(path.clone(), rules.clone());
        }

        let mut documentation = BTreeMap::new();
        for (path, doc) in &schema.documentation {
            documentation.insert(path.clone(), doc.clone());
        }

        let mut ai_tags = BTreeMap::new();
        for (name, tag) in &schema.ai_tags {
            ai_tags.insert(name.clone(), tag.clone());
        }

        let mut sensitive_paths = BTreeMap::new();
        for (path, flagged) in &schema.sensitive_paths {
            if *flagged {
                sensitive_paths.insert(path.clone(), true);
            }
        }

        Self {
            version: schema.version.clone(),
            types,
            root: schema.root.clone(),
            constraints,
            documentation,
            ai_tags,
            sensitive_paths,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
enum CanonicalTypeDefinition {
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    Bool,
    String,
    Bytes,
    Null,
    List(Box<CanonicalTypeDefinition>),
    Map(Box<CanonicalTypeDefinition>, Box<CanonicalTypeDefinition>),
    Struct(BTreeMap<String, CanonicalFieldDefinition>),
    Union(Vec<CanonicalVariantDefinition>),
    Optional(Box<CanonicalTypeDefinition>),
    Custom(String),
}

impl CanonicalTypeDefinition {
    fn from_type(type_def: &TypeDefinition) -> Self {
        match type_def {
            TypeDefinition::Int8 => Self::Int8,
            TypeDefinition::Int16 => Self::Int16,
            TypeDefinition::Int32 => Self::Int32,
            TypeDefinition::Int64 => Self::Int64,
            TypeDefinition::UInt8 => Self::UInt8,
            TypeDefinition::UInt16 => Self::UInt16,
            TypeDefinition::UInt32 => Self::UInt32,
            TypeDefinition::UInt64 => Self::UInt64,
            TypeDefinition::Float32 => Self::Float32,
            TypeDefinition::Float64 => Self::Float64,
            TypeDefinition::Bool => Self::Bool,
            TypeDefinition::String => Self::String,
            TypeDefinition::Bytes => Self::Bytes,
            TypeDefinition::Null => Self::Null,
            TypeDefinition::List(inner) => Self::List(Box::new(Self::from_type(inner))),
            TypeDefinition::Map(key, value) => Self::Map(
                Box::new(Self::from_type(key)),
                Box::new(Self::from_type(value)),
            ),
            TypeDefinition::Struct(fields) => {
                let mut sorted = BTreeMap::new();
                for (name, def) in fields {
                    sorted.insert(name.clone(), CanonicalFieldDefinition::from_field(def));
                }
                Self::Struct(sorted)
            }
            TypeDefinition::Union(variants) => Self::Union(
                variants
                    .iter()
                    .map(CanonicalVariantDefinition::from_variant)
                    .collect(),
            ),
            TypeDefinition::Optional(inner) => Self::Optional(Box::new(Self::from_type(inner))),
            TypeDefinition::Custom(name) => Self::Custom(name.clone()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct CanonicalFieldDefinition {
    field_type: CanonicalTypeDefinition,
    required: bool,
    default: Option<Value>,
    constraints: Vec<Constraint>,
    documentation: Option<String>,
    ai_tag: Option<String>,
}

impl CanonicalFieldDefinition {
    fn from_field(field: &FieldDefinition) -> Self {
        Self {
            field_type: CanonicalTypeDefinition::from_type(&field.field_type),
            required: field.required,
            default: field.default.clone(),
            constraints: field.constraints.clone(),
            documentation: field.documentation.clone(),
            ai_tag: field.ai_tag.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct CanonicalVariantDefinition {
    name: String,
    tag: u32,
    value_type: CanonicalTypeDefinition,
    documentation: Option<String>,
}

impl CanonicalVariantDefinition {
    fn from_variant(variant: &VariantDefinition) -> Self {
        Self {
            name: variant.name.clone(),
            tag: variant.tag,
            value_type: CanonicalTypeDefinition::from_type(&variant.value_type),
            documentation: variant.documentation.clone(),
        }
    }
}

// ============================================================================
// Schema Engine
// ============================================================================

/// Validation error with path information
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
}

impl ValidationError {
    pub fn new(path: String, message: String) -> Self {
        Self { path, message }
    }
}

/// Validation result containing all errors
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub errors: Vec<ValidationError>,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn add_error(&mut self, path: String, message: String) {
        self.errors.push(ValidationError::new(path, message));
    }

    pub fn merge(&mut self, other: ValidationResult) {
        self.errors.extend(other.errors);
    }
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Schema engine for validation and type operations
pub struct SchemaEngine {
    /// Registered custom types
    custom_types: HashMap<String, TypeDefinition>,
}

impl SchemaEngine {
    /// Create a new schema engine
    pub fn new() -> Self {
        Self {
            custom_types: HashMap::new(),
        }
    }

    /// Register a custom type
    pub fn register_custom_type(&mut self, name: String, type_def: TypeDefinition) {
        self.custom_types.insert(name, type_def);
    }

    /// Get a custom type by name
    pub fn get_custom_type(&self, name: &str) -> Option<&TypeDefinition> {
        self.custom_types.get(name)
    }

    /// Check if a custom type exists
    pub fn has_custom_type(&self, name: &str) -> bool {
        self.custom_types.contains_key(name)
    }

    /// Resolve a custom type recursively (follows Custom type references)
    pub fn resolve_custom_type(
        &self,
        type_def: &TypeDefinition,
        schema: &Schema,
    ) -> Result<TypeDefinition> {
        match type_def {
            TypeDefinition::Custom(name) => {
                // Try to find in engine's custom types first
                if let Some(custom_type) = self.get_custom_type(name) {
                    // Recursively resolve in case it references another custom type
                    return self.resolve_custom_type(custom_type, schema);
                }

                // Try to find in schema's types
                if let Some(schema_type) = schema.get_type(name) {
                    // Recursively resolve in case it references another custom type
                    return self.resolve_custom_type(schema_type, schema);
                }

                Err(BCSError::Validation(format!(
                    "Custom type '{}' not found",
                    name
                )))
            }
            // For non-custom types, return as-is
            _ => Ok(type_def.clone()),
        }
    }

    /// Compose a new type from existing types (type composition)
    pub fn compose_type(
        &mut self,
        name: String,
        base_types: Vec<String>,
        schema: &Schema,
    ) -> Result<()> {
        // Resolve all base types
        let mut resolved_types = Vec::new();
        for base_name in &base_types {
            let base_type = TypeDefinition::Custom(base_name.clone());
            let resolved = self.resolve_custom_type(&base_type, schema)?;
            resolved_types.push(resolved);
        }

        // Compose types - for now, we'll merge struct fields
        let mut composed_fields = HashMap::new();

        for resolved_type in resolved_types {
            match resolved_type {
                TypeDefinition::Struct(fields) => {
                    // Merge fields from this struct
                    for (field_name, field_def) in fields {
                        composed_fields.insert(field_name, field_def);
                    }
                }
                _ => {
                    return Err(BCSError::Validation(format!(
                        "Cannot compose non-struct type in '{}'",
                        name
                    )));
                }
            }
        }

        // Register the composed type
        let composed_type = TypeDefinition::Struct(composed_fields);
        self.register_custom_type(name, composed_type);

        Ok(())
    }

    /// Extend a type with additional fields (type inheritance)
    pub fn extend_type(
        &mut self,
        name: String,
        base_type: String,
        additional_fields: HashMap<String, FieldDefinition>,
        schema: &Schema,
    ) -> Result<()> {
        // Resolve the base type
        let base_type_def = TypeDefinition::Custom(base_type.clone());
        let resolved_base = self.resolve_custom_type(&base_type_def, schema)?;

        // Ensure base is a struct
        match resolved_base {
            TypeDefinition::Struct(mut base_fields) => {
                // Add additional fields
                for (field_name, field_def) in additional_fields {
                    base_fields.insert(field_name, field_def);
                }

                // Register the extended type
                let extended_type = TypeDefinition::Struct(base_fields);
                self.register_custom_type(name, extended_type);

                Ok(())
            }
            _ => Err(BCSError::Validation(format!(
                "Cannot extend non-struct type '{}'",
                base_type
            ))),
        }
    }

    /// Validate a custom type definition recursively
    pub fn validate_custom_type(
        &self,
        type_def: &TypeDefinition,
        schema: &Schema,
        visited: &mut std::collections::HashSet<String>,
    ) -> Result<()> {
        match type_def {
            TypeDefinition::Custom(name) => {
                // Check for circular references
                if visited.contains(name) {
                    return Err(BCSError::Validation(format!(
                        "Circular type reference detected: {}",
                        name
                    )));
                }

                visited.insert(name.clone());

                // Resolve and validate the custom type
                let resolved = self.resolve_custom_type(type_def, schema)?;
                self.validate_custom_type(&resolved, schema, visited)?;

                visited.remove(name);
                Ok(())
            }
            TypeDefinition::List(inner) => self.validate_custom_type(inner, schema, visited),
            TypeDefinition::Map(key_type, value_type) => {
                self.validate_custom_type(key_type, schema, visited)?;
                self.validate_custom_type(value_type, schema, visited)
            }
            TypeDefinition::Struct(fields) => {
                for field_def in fields.values() {
                    self.validate_custom_type(&field_def.field_type, schema, visited)?;
                }
                Ok(())
            }
            TypeDefinition::Union(variants) => {
                for variant in variants {
                    self.validate_custom_type(&variant.value_type, schema, visited)?;
                }
                Ok(())
            }
            TypeDefinition::Optional(inner) => self.validate_custom_type(inner, schema, visited),
            _ => {
                // Primitive types are always valid
                Ok(())
            }
        }
    }

    /// Validate a value against a schema
    pub fn validate(&self, value: &Value, schema: &Schema) -> ValidationResult {
        let mut result = ValidationResult::new();
        let mut visited = std::collections::HashSet::new();

        // Get root type
        match schema.get_root_type() {
            Ok(root_type) => {
                self.validate_value(value, root_type, schema, "", &mut result, &mut visited, 0);
            }
            Err(e) => {
                result.add_error("".to_string(), format!("Schema error: {}", e));
            }
        }

        result
    }

    /// Validate a value against a type definition
    #[allow(clippy::too_many_arguments)]
    fn validate_value(
        &self,
        value: &Value,
        type_def: &TypeDefinition,
        schema: &Schema,
        path: &str,
        result: &mut ValidationResult,
        visited: &mut std::collections::HashSet<String>,
        depth: usize,
    ) {
        if crate::limits::ensure_depth(depth).is_err() {
            result.add_error(
                path.to_string(),
                format!(
                    "Nesting depth exceeds limit {}",
                    crate::limits::MAX_NESTING_DEPTH
                ),
            );
            return;
        }
        match (value, type_def) {
            // Null
            (Value::Null, TypeDefinition::Null) => {}

            // Boolean
            (Value::Bool(_), TypeDefinition::Bool) => {}

            // Integers
            (Value::Int8(_), TypeDefinition::Int8) => {}
            (Value::Int16(_), TypeDefinition::Int16) => {}
            (Value::Int32(_), TypeDefinition::Int32) => {}
            (Value::Int64(_), TypeDefinition::Int64) => {}
            (Value::UInt8(_), TypeDefinition::UInt8) => {}
            (Value::UInt16(_), TypeDefinition::UInt16) => {}
            (Value::UInt32(_), TypeDefinition::UInt32) => {}
            (Value::UInt64(_), TypeDefinition::UInt64) => {}

            // Floats
            (Value::Float32(_), TypeDefinition::Float32) => {}
            (Value::Float64(_), TypeDefinition::Float64) => {}

            // String
            (Value::String(_), TypeDefinition::String) => {}

            // Bytes
            (Value::Bytes(_), TypeDefinition::Bytes) => {}

            // List
            (Value::List(items), TypeDefinition::List(item_type)) => {
                for (i, item) in items.iter().enumerate() {
                    let item_path = format!("{}[{}]", path, i);
                    self.validate_value(
                        item,
                        item_type,
                        schema,
                        &item_path,
                        result,
                        visited,
                        depth + 1,
                    );
                }
            }

            // Map
            (Value::Map(entries), TypeDefinition::Map(key_type, value_type)) => {
                for (i, (key, val)) in entries.iter().enumerate() {
                    let key_path = format!("{}[{}].key", path, i);
                    let val_path = format!("{}[{}].value", path, i);
                    self.validate_value(
                        key,
                        key_type,
                        schema,
                        &key_path,
                        result,
                        visited,
                        depth + 1,
                    );
                    self.validate_value(
                        val,
                        value_type,
                        schema,
                        &val_path,
                        result,
                        visited,
                        depth + 1,
                    );
                }
            }

            // Struct
            (Value::Struct(fields), TypeDefinition::Struct(field_defs)) => {
                self.validate_struct(fields, field_defs, schema, path, result, visited, depth);
            }

            // Union
            (Value::Union(tag, val), TypeDefinition::Union(variants)) => {
                self.validate_union(*tag, val, variants, schema, path, result, visited, depth);
            }

            // Optional
            (Value::Optional(None), TypeDefinition::Optional(_)) => {}
            (Value::Optional(Some(val)), TypeDefinition::Optional(inner_type)) => {
                self.validate_value(val, inner_type, schema, path, result, visited, depth + 1);
            }

            // Custom type
            (_, TypeDefinition::Custom(custom_name)) => {
                if visited.contains(custom_name) {
                    result.add_error(
                        path.to_string(),
                        format!("Circular type reference detected: {}", custom_name),
                    );
                    return;
                }

                visited.insert(custom_name.clone());
                if let Some(custom_type) = self.get_custom_type(custom_name) {
                    self.validate_value(value, custom_type, schema, path, result, visited, depth);
                } else if let Some(custom_type) = schema.get_type(custom_name) {
                    self.validate_value(value, custom_type, schema, path, result, visited, depth);
                } else {
                    result.add_error(
                        path.to_string(),
                        format!("Unknown custom type: {}", custom_name),
                    );
                }
                visited.remove(custom_name);
            }

            // Type mismatch
            _ => {
                result.add_error(
                    path.to_string(),
                    format!("Type mismatch: expected {:?}, got {:?}", type_def, value),
                );
            }
        }

        // Validate constraints
        if let Some(constraints) = schema.get_constraints(path) {
            self.validate_constraints(value, constraints, path, result);
        }
    }

    /// Validate struct fields
    #[allow(clippy::too_many_arguments)]
    fn validate_struct(
        &self,
        fields: &[(String, u64, Value)],
        field_defs: &HashMap<String, FieldDefinition>,
        schema: &Schema,
        path: &str,
        result: &mut ValidationResult,
        visited: &mut std::collections::HashSet<String>,
        depth: usize,
    ) {
        // Create a map of field names to values for easier lookup
        let field_map: HashMap<&String, &Value> =
            fields.iter().map(|(name, _h, v)| (name, v)).collect();

        // Check required fields and validate present fields
        for (field_name, field_def) in field_defs {
            let field_path = if path.is_empty() {
                field_name.clone()
            } else {
                format!("{}.{}", path, field_name)
            };

            match field_map.get(field_name) {
                Some(value) => {
                    // Field is present, validate it
                    self.validate_value(
                        value,
                        &field_def.field_type,
                        schema,
                        &field_path,
                        result,
                        visited,
                        depth + 1,
                    );

                    // Validate field-specific constraints
                    self.validate_constraints(value, &field_def.constraints, &field_path, result);
                }
                None => {
                    // Field is missing
                    if field_def.required && field_def.default.is_none() {
                        result.add_error(
                            field_path,
                            format!("Required field '{}' is missing", field_name),
                        );
                    }
                }
            }
        }
    }

    /// Validate union variant
    #[allow(clippy::too_many_arguments)]
    fn validate_union(
        &self,
        tag: u32,
        value: &Value,
        variants: &[VariantDefinition],
        schema: &Schema,
        path: &str,
        result: &mut ValidationResult,
        visited: &mut std::collections::HashSet<String>,
        depth: usize,
    ) {
        // Find the variant with matching tag
        if let Some(variant) = variants.iter().find(|v| v.tag == tag) {
            let variant_path = format!("{}::{}", path, variant.name);
            self.validate_value(
                value,
                &variant.value_type,
                schema,
                &variant_path,
                result,
                visited,
                depth + 1,
            );
        } else {
            result.add_error(
                path.to_string(),
                format!("Unknown union variant tag: {}", tag),
            );
        }
    }

    /// Validate constraints
    fn validate_constraints(
        &self,
        value: &Value,
        constraints: &[Constraint],
        path: &str,
        result: &mut ValidationResult,
    ) {
        for constraint in constraints {
            match constraint {
                Constraint::Range(min, max) => {
                    self.validate_range(value, *min, *max, path, result);
                }
                Constraint::Pattern(pattern) => {
                    self.validate_pattern(value, pattern, path, result);
                }
                Constraint::NonEmpty => {
                    self.validate_non_empty(value, path, result);
                }
                Constraint::Length(min, max) => {
                    self.validate_length(value, *min, *max, path, result);
                }
                Constraint::Unique => {
                    self.validate_unique(value, path, result);
                }
                Constraint::Enum(allowed_values) => {
                    self.validate_enum(value, allowed_values, path, result);
                }
                Constraint::Min(min) => {
                    self.validate_min(value, *min, path, result);
                }
                Constraint::Max(max) => {
                    self.validate_max(value, *max, path, result);
                }
                Constraint::Custom(expr) => {
                    self.validate_custom_constraint(value, expr, path, result);
                }
            }
        }
    }

    /// Validate custom constraint expressions.
    ///
    /// Supported forms (fail-closed for unknown expressions):
    /// - `non_empty`
    /// - `min:<number>` / `max:<number>`
    /// - `range:<min>:<max>`
    /// - `length:<min>:<max>` (`-` for open bound)
    /// - `pattern:<regex>`
    /// - `enum:<a>|<b>|<c>`
    fn validate_custom_constraint(
        &self,
        value: &Value,
        expr: &str,
        path: &str,
        result: &mut ValidationResult,
    ) {
        let expr = expr.trim();
        if expr.is_empty() {
            result.add_error(path.to_string(), "Empty custom constraint".to_string());
            return;
        }

        if expr.eq_ignore_ascii_case("non_empty") {
            self.validate_non_empty(value, path, result);
            return;
        }

        if let Some(rest) = expr.strip_prefix("min:") {
            if let Ok(min) = rest.parse::<f64>() {
                self.validate_min(value, min, path, result);
            } else {
                result.add_error(
                    path.to_string(),
                    format!("Invalid custom min constraint: '{}'", expr),
                );
            }
            return;
        }

        if let Some(rest) = expr.strip_prefix("max:") {
            if let Ok(max) = rest.parse::<f64>() {
                self.validate_max(value, max, path, result);
            } else {
                result.add_error(
                    path.to_string(),
                    format!("Invalid custom max constraint: '{}'", expr),
                );
            }
            return;
        }

        if let Some(rest) = expr.strip_prefix("range:") {
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() == 2 {
                if let (Ok(min), Ok(max)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    self.validate_range(value, min, max, path, result);
                    return;
                }
            }
            result.add_error(
                path.to_string(),
                format!("Invalid custom range constraint: '{}'", expr),
            );
            return;
        }

        if let Some(rest) = expr.strip_prefix("length:") {
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() == 2 {
                let min = if parts[0] == "-" {
                    None
                } else {
                    parts[0].parse::<usize>().ok()
                };
                let max = if parts[1] == "-" {
                    None
                } else {
                    parts[1].parse::<usize>().ok()
                };
                if min.is_some() || max.is_some() || (parts[0] == "-" && parts[1] == "-") {
                    self.validate_length(value, min, max, path, result);
                    return;
                }
            }
            result.add_error(
                path.to_string(),
                format!("Invalid custom length constraint: '{}'", expr),
            );
            return;
        }

        if let Some(pattern) = expr.strip_prefix("pattern:") {
            self.validate_pattern(value, pattern, path, result);
            return;
        }

        if let Some(rest) = expr.strip_prefix("enum:") {
            let allowed: Vec<Value> = rest
                .split('|')
                .map(|s| Value::String(s.to_string()))
                .collect();
            self.validate_enum(value, &allowed, path, result);
            return;
        }

        result.add_error(
            path.to_string(),
            format!(
                "Unsupported custom constraint '{}'. Use non_empty, min:, max:, range:, length:, pattern:, or enum:",
                expr
            ),
        );
    }

    /// Validate range constraint
    fn validate_range(
        &self,
        value: &Value,
        min: f64,
        max: f64,
        path: &str,
        result: &mut ValidationResult,
    ) {
        let num_value = match value {
            Value::Int8(v) => Some(*v as f64),
            Value::Int16(v) => Some(*v as f64),
            Value::Int32(v) => Some(*v as f64),
            Value::Int64(v) => Some(*v as f64),
            Value::UInt8(v) => Some(*v as f64),
            Value::UInt16(v) => Some(*v as f64),
            Value::UInt32(v) => Some(*v as f64),
            Value::UInt64(v) => Some(*v as f64),
            Value::Float32(v) => Some(*v as f64),
            Value::Float64(v) => Some(*v),
            _ => None,
        };

        if let Some(num) = num_value {
            if num < min || num > max {
                result.add_error(
                    path.to_string(),
                    format!("Value {} is outside range [{}, {}]", num, min, max),
                );
            }
        }
    }

    /// Validate pattern constraint (regex)
    fn validate_pattern(
        &self,
        value: &Value,
        pattern: &str,
        path: &str,
        result: &mut ValidationResult,
    ) {
        if let Value::String(s) = value {
            match Regex::new(pattern) {
                Ok(re) => {
                    if !re.is_match(s) {
                        result.add_error(
                            path.to_string(),
                            format!("String '{}' does not match pattern '{}'", s, pattern),
                        );
                    }
                }
                Err(e) => {
                    result.add_error(
                        path.to_string(),
                        format!("Invalid regex pattern '{}': {}", pattern, e),
                    );
                }
            }
        }
    }

    /// Validate non-empty constraint
    fn validate_non_empty(&self, value: &Value, path: &str, result: &mut ValidationResult) {
        let is_empty = match value {
            Value::String(s) => s.is_empty(),
            Value::Bytes(b) => b.is_empty(),
            Value::List(l) => l.is_empty(),
            Value::Map(m) => m.is_empty(),
            _ => false,
        };

        if is_empty {
            result.add_error(path.to_string(), "Value must not be empty".to_string());
        }
    }

    /// Validate length constraint
    fn validate_length(
        &self,
        value: &Value,
        min: Option<usize>,
        max: Option<usize>,
        path: &str,
        result: &mut ValidationResult,
    ) {
        let length = match value {
            Value::String(s) => Some(s.len()),
            Value::Bytes(b) => Some(b.len()),
            Value::List(l) => Some(l.len()),
            Value::Map(m) => Some(m.len()),
            _ => None,
        };

        if let Some(len) = length {
            if let Some(min_len) = min {
                if len < min_len {
                    result.add_error(
                        path.to_string(),
                        format!("Length {} is less than minimum {}", len, min_len),
                    );
                }
            }
            if let Some(max_len) = max {
                if len > max_len {
                    result.add_error(
                        path.to_string(),
                        format!("Length {} exceeds maximum {}", len, max_len),
                    );
                }
            }
        }
    }

    /// Validate unique constraint
    fn validate_unique(&self, value: &Value, path: &str, result: &mut ValidationResult) {
        if let Value::List(items) = value {
            let mut seen = std::collections::HashSet::new();
            for (i, item) in items.iter().enumerate() {
                let item_str = format!("{:?}", item);
                if !seen.insert(item_str.clone()) {
                    result.add_error(
                        format!("{}[{}]", path, i),
                        format!("Duplicate value: {}", item_str),
                    );
                }
            }
        }
    }

    /// Validate enum constraint
    fn validate_enum(
        &self,
        value: &Value,
        allowed_values: &[Value],
        path: &str,
        result: &mut ValidationResult,
    ) {
        if !allowed_values.contains(value) {
            result.add_error(
                path.to_string(),
                format!("Value must be one of: {:?}", allowed_values),
            );
        }
    }

    /// Validate minimum constraint
    fn validate_min(&self, value: &Value, min: f64, path: &str, result: &mut ValidationResult) {
        let num_value = match value {
            Value::Int8(v) => Some(*v as f64),
            Value::Int16(v) => Some(*v as f64),
            Value::Int32(v) => Some(*v as f64),
            Value::Int64(v) => Some(*v as f64),
            Value::UInt8(v) => Some(*v as f64),
            Value::UInt16(v) => Some(*v as f64),
            Value::UInt32(v) => Some(*v as f64),
            Value::UInt64(v) => Some(*v as f64),
            Value::Float32(v) => Some(*v as f64),
            Value::Float64(v) => Some(*v),
            _ => None,
        };

        if let Some(num) = num_value {
            if num < min {
                result.add_error(
                    path.to_string(),
                    format!("Value {} is less than minimum {}", num, min),
                );
            }
        }
    }

    /// Validate maximum constraint
    fn validate_max(&self, value: &Value, max: f64, path: &str, result: &mut ValidationResult) {
        let num_value = match value {
            Value::Int8(v) => Some(*v as f64),
            Value::Int16(v) => Some(*v as f64),
            Value::Int32(v) => Some(*v as f64),
            Value::Int64(v) => Some(*v as f64),
            Value::UInt8(v) => Some(*v as f64),
            Value::UInt16(v) => Some(*v as f64),
            Value::UInt32(v) => Some(*v as f64),
            Value::UInt64(v) => Some(*v as f64),
            Value::Float32(v) => Some(*v as f64),
            Value::Float64(v) => Some(*v),
            _ => None,
        };

        if let Some(num) = num_value {
            if num > max {
                result.add_error(
                    path.to_string(),
                    format!("Value {} exceeds maximum {}", num, max),
                );
            }
        }
    }

    /// Hash a field name (simple implementation using xxhash)
    fn hash_field_name(name: &str) -> u64 {
        use xxhash_rust::xxh64::xxh64;
        xxh64(name.as_bytes(), 0)
    }

    /// Apply default values to a value based on schema
    pub fn apply_defaults(&self, value: &mut Value, schema: &Schema) -> Result<()> {
        match schema.get_root_type() {
            Ok(root_type) => {
                self.apply_defaults_to_value(value, root_type, schema)?;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Apply default values to a value recursively
    fn apply_defaults_to_value(
        &self,
        value: &mut Value,
        type_def: &TypeDefinition,
        schema: &Schema,
    ) -> Result<()> {
        match type_def {
            TypeDefinition::Struct(field_defs) => {
                if let Value::Struct(fields) = value {
                    // Collect existing field names
                    let existing_names: std::collections::HashSet<&String> =
                        fields.iter().map(|(name, _h, _v)| name).collect();

                    // Collect defaults for missing fields
                    let mut defaults_to_add = Vec::new();
                    for (field_name, field_def) in field_defs {
                        if !existing_names.contains(field_name) {
                            if let Some(default_value) = &field_def.default {
                                let field_hash = Self::hash_field_name(field_name);
                                defaults_to_add.push((
                                    field_name.clone(),
                                    field_hash,
                                    default_value.clone(),
                                ));
                            }
                        }
                    }

                    // Add defaults
                    fields.extend(defaults_to_add);
                }
            }
            TypeDefinition::Optional(inner_type) => {
                if let Value::Optional(Some(val)) = value {
                    self.apply_defaults_to_value(val, inner_type, schema)?;
                }
            }
            TypeDefinition::Custom(custom_name) => {
                if let Some(custom_type) = self.get_custom_type(custom_name) {
                    self.apply_defaults_to_value(value, custom_type, schema)?;
                } else if let Some(custom_type) = schema.get_type(custom_name) {
                    self.apply_defaults_to_value(value, custom_type, schema)?;
                }
            }
            _ => {}
        }

        Ok(())
    }
}

impl Default for SchemaEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Schema Parser
// ============================================================================

/// Schema parser for building schemas from semantic layer
pub struct SchemaParser {
    engine: SchemaEngine,
}

impl SchemaParser {
    /// Create a new schema parser
    pub fn new() -> Self {
        Self {
            engine: SchemaEngine::new(),
        }
    }

    /// Parse schema from MessagePack-encoded semantic layer
    pub fn parse(&mut self, data: &[u8]) -> Result<Schema> {
        // Decode the schema from MessagePack
        let schema = Schema::from_msgpack(data)?;

        // Build type registry by registering all custom types
        self.build_type_registry(&schema)?;

        // Validate the schema structure
        self.validate_schema(&schema)?;

        Ok(schema)
    }

    /// Parse schema from a reader
    pub fn parse_reader<R: Read>(&mut self, reader: R) -> Result<Schema> {
        // Decode the schema from MessagePack
        let schema = Schema::from_msgpack_reader(reader)?;

        // Build type registry by registering all custom types
        self.build_type_registry(&schema)?;

        // Validate the schema structure
        self.validate_schema(&schema)?;

        Ok(schema)
    }

    /// Build type registry from schema
    fn build_type_registry(&mut self, schema: &Schema) -> Result<()> {
        // Register all types as custom types in the engine
        for (name, type_def) in &schema.types {
            self.engine
                .register_custom_type(name.clone(), type_def.clone());
        }

        Ok(())
    }

    /// Validate schema structure
    fn validate_schema(&self, schema: &Schema) -> Result<()> {
        // Check that root type exists
        if !schema.types.contains_key(&schema.root) {
            return Err(BCSError::Validation(format!(
                "Root type '{}' not found in schema",
                schema.root
            )));
        }

        // Validate all type definitions
        for (name, type_def) in &schema.types {
            self.validate_type_definition(name, type_def, schema)?;
        }

        Ok(())
    }

    /// Validate a type definition
    fn validate_type_definition(
        &self,
        name: &str,
        type_def: &TypeDefinition,
        schema: &Schema,
    ) -> Result<()> {
        match type_def {
            TypeDefinition::Custom(custom_name) => {
                // Check that custom type exists
                if !schema.types.contains_key(custom_name) {
                    return Err(BCSError::Validation(format!(
                        "Type '{}' references undefined custom type '{}'",
                        name, custom_name
                    )));
                }
            }
            TypeDefinition::List(inner) => {
                self.validate_type_definition(name, inner, schema)?;
            }
            TypeDefinition::Map(key_type, value_type) => {
                self.validate_type_definition(name, key_type, schema)?;
                self.validate_type_definition(name, value_type, schema)?;
            }
            TypeDefinition::Struct(fields) => {
                for (field_name, field_def) in fields {
                    self.validate_type_definition(
                        &format!("{}.{}", name, field_name),
                        &field_def.field_type,
                        schema,
                    )?;
                }
            }
            TypeDefinition::Union(variants) => {
                for variant in variants {
                    self.validate_type_definition(
                        &format!("{}::{}", name, variant.name),
                        &variant.value_type,
                        schema,
                    )?;
                }
            }
            TypeDefinition::Optional(inner) => {
                self.validate_type_definition(name, inner, schema)?;
            }
            _ => {
                // Primitive types are always valid
            }
        }

        Ok(())
    }

    /// Get the schema engine
    pub fn engine(&self) -> &SchemaEngine {
        &self.engine
    }

    /// Get mutable reference to the schema engine
    pub fn engine_mut(&mut self) -> &mut SchemaEngine {
        &mut self.engine
    }
}

impl Default for SchemaParser {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Agent-safe schema export + sensitive plaintext policy
// ============================================================================

/// One field (or path) in an agent-safe schema export. Never includes data-layer values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSafePath {
    pub path: String,
    pub type_name: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
    pub sensitive: bool,
}

/// Agent-safe schema contract (no secret values).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSafeSchema {
    pub version: String,
    pub root: String,
    pub paths: Vec<AgentSafePath>,
    pub sensitive_paths: Vec<String>,
}

impl Schema {
    /// Build an agent-safe view: paths, types, docs, sensitivity — never field values.
    pub fn to_agent_safe(&self) -> AgentSafeSchema {
        let mut paths = Vec::new();
        if let Ok(root_type) = self.get_root_type() {
            collect_agent_safe_paths(self, root_type, "", &mut paths);
        }
        // Ensure every explicitly marked sensitive path appears even if type walk missed it.
        for path in self.sensitive_path_list() {
            if !paths.iter().any(|p| p.path == path) {
                paths.push(AgentSafePath {
                    path: path.clone(),
                    type_name: "unknown".to_string(),
                    required: false,
                    documentation: self.get_documentation(&path).cloned(),
                    constraints: self
                        .get_constraints(&path)
                        .map(|c| c.iter().map(|x| format!("{:?}", x)).collect())
                        .unwrap_or_default(),
                    sensitive: true,
                });
            }
        }
        paths.sort_by(|a, b| a.path.cmp(&b.path));
        let sensitive_paths = self.sensitive_path_list();
        AgentSafeSchema {
            version: self.version.clone(),
            root: self.root.clone(),
            paths,
            sensitive_paths,
        }
    }

    /// Serialize agent-safe schema to pretty JSON.
    pub fn to_agent_safe_json(&self) -> Result<String> {
        serde_json::to_string_pretty(&self.to_agent_safe()).map_err(|e| {
            BCSError::Encoding(format!("Failed to serialize agent-safe schema: {}", e))
        })
    }
}

fn type_definition_name(type_def: &TypeDefinition) -> String {
    match type_def {
        TypeDefinition::Int8 => "int8".into(),
        TypeDefinition::Int16 => "int16".into(),
        TypeDefinition::Int32 => "int32".into(),
        TypeDefinition::Int64 => "int64".into(),
        TypeDefinition::UInt8 => "uint8".into(),
        TypeDefinition::UInt16 => "uint16".into(),
        TypeDefinition::UInt32 => "uint32".into(),
        TypeDefinition::UInt64 => "uint64".into(),
        TypeDefinition::Float32 => "float32".into(),
        TypeDefinition::Float64 => "float64".into(),
        TypeDefinition::Bool => "bool".into(),
        TypeDefinition::String => "string".into(),
        TypeDefinition::Bytes => "bytes".into(),
        TypeDefinition::Null => "null".into(),
        TypeDefinition::List(inner) => format!("list<{}>", type_definition_name(inner)),
        TypeDefinition::Map(k, v) => format!(
            "map<{},{}>",
            type_definition_name(k),
            type_definition_name(v)
        ),
        TypeDefinition::Struct(_) => "struct".into(),
        TypeDefinition::Union(_) => "union".into(),
        TypeDefinition::Optional(inner) => format!("optional<{}>", type_definition_name(inner)),
        TypeDefinition::Custom(name) => name.clone(),
    }
}

fn collect_agent_safe_paths(
    schema: &Schema,
    type_def: &TypeDefinition,
    path: &str,
    out: &mut Vec<AgentSafePath>,
) {
    match type_def {
        TypeDefinition::Struct(fields) => {
            for (field_name, field_def) in fields {
                let field_path = if path.is_empty() {
                    field_name.clone()
                } else {
                    format!("{}.{}", path, field_name)
                };
                let docs = field_def
                    .documentation
                    .clone()
                    .or_else(|| schema.get_documentation(&field_path).cloned());
                let mut constraints: Vec<String> = field_def
                    .constraints
                    .iter()
                    .map(|c| format!("{:?}", c))
                    .collect();
                if let Some(path_constraints) = schema.get_constraints(&field_path) {
                    for c in path_constraints {
                        constraints.push(format!("{:?}", c));
                    }
                }
                out.push(AgentSafePath {
                    path: field_path.clone(),
                    type_name: type_definition_name(&field_def.field_type),
                    required: field_def.required,
                    documentation: docs,
                    constraints,
                    sensitive: schema.is_sensitive(&field_path),
                });
                collect_agent_safe_paths(schema, &field_def.field_type, &field_path, out);
            }
        }
        TypeDefinition::List(inner) => {
            let item_path = format!("{}[]", path);
            collect_agent_safe_paths(schema, inner, &item_path, out);
        }
        TypeDefinition::Optional(inner) => {
            collect_agent_safe_paths(schema, inner, path, out);
        }
        TypeDefinition::Custom(name) => {
            if let Some(resolved) = schema.get_type(name) {
                collect_agent_safe_paths(schema, resolved, path, out);
            }
        }
        TypeDefinition::Map(_, value_type) => {
            let entry_path = format!("{}.*", path);
            collect_agent_safe_paths(schema, value_type, &entry_path, out);
        }
        TypeDefinition::Union(variants) => {
            for variant in variants {
                let variant_path = format!("{}::{}", path, variant.name);
                collect_agent_safe_paths(schema, &variant.value_type, &variant_path, out);
            }
        }
        _ => {}
    }
}

/// Placeholder written by [`redact_sensitive_plaintext`] for schema-marked plaintext.
pub const SENSITIVE_PLAINTEXT_MASK: &str = "[SENSITIVE]";

/// Finding: a path marked sensitive still holds plaintext (not protect/secret-ref marker).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivePlaintextFinding {
    pub path: String,
    pub message: String,
}

/// Check sensitive paths for plaintext values. Protect markers and secret refs are OK.
pub fn find_sensitive_plaintext(
    schema: &Schema,
    value: &Value,
) -> Result<Vec<SensitivePlaintextFinding>> {
    find_sensitive_plaintext_under(schema, value, None)
}

/// Like [`find_sensitive_plaintext`], but `value` is rooted at `root_path` (e.g. a
/// `decode --path` subtree). Pass `None` when `value` is the document root.
pub fn find_sensitive_plaintext_under(
    schema: &Schema,
    value: &Value,
    root_path: Option<&str>,
) -> Result<Vec<SensitivePlaintextFinding>> {
    use crate::index::parse_path;

    let mut findings = Vec::new();
    for path in schema.sensitive_path_list() {
        let relative = match relative_sensitive_path(&path, root_path) {
            Some(rel) => rel,
            None => continue,
        };
        let segments = if relative.is_empty() {
            Vec::new()
        } else {
            parse_path(relative)?
        };
        match get_value_at_segments(value, &segments) {
            Some(leaf) => {
                if is_protected_or_secret_or_mask(leaf) {
                    continue;
                }
                findings.push(SensitivePlaintextFinding {
                    path: path.clone(),
                    message: format!(
                        "Path '{}' is marked sensitive but holds plaintext (not a protect marker or secret ref)",
                        path
                    ),
                });
            }
            None => {
                // Path missing from data — not a plaintext leak.
            }
        }
    }
    Ok(findings)
}

/// True for encrypt/ref markers **or** post-mask placeholders (`[PROTECTED]`, etc.).
/// Decode/show run plaintext policy after masking, so placeholders must not fail CI.
fn is_protected_or_secret_or_mask(leaf: &Value) -> bool {
    use crate::security::{is_protected_marker, is_secret_ref_marker};
    if is_protected_marker(leaf) || is_secret_ref_marker(leaf) {
        return true;
    }
    matches!(
        leaf,
        Value::String(s)
            if s == "[PROTECTED]"
                || s == "[SECRET_REF]"
                || s == SENSITIVE_PLAINTEXT_MASK
    )
}

/// Replace sensitive plaintext leaves with [`SENSITIVE_PLAINTEXT_MASK`].
/// Returns the same findings that would be reported by [`find_sensitive_plaintext_under`].
pub fn redact_sensitive_plaintext(
    schema: &Schema,
    value: &mut Value,
) -> Result<Vec<SensitivePlaintextFinding>> {
    redact_sensitive_plaintext_under(schema, value, None)
}

/// Like [`redact_sensitive_plaintext`] for a value rooted at `root_path`.
pub fn redact_sensitive_plaintext_under(
    schema: &Schema,
    value: &mut Value,
    root_path: Option<&str>,
) -> Result<Vec<SensitivePlaintextFinding>> {
    use crate::index::parse_path;

    let findings = find_sensitive_plaintext_under(schema, value, root_path)?;
    for finding in &findings {
        let relative = match relative_sensitive_path(&finding.path, root_path) {
            Some(rel) => rel,
            None => continue,
        };
        let segments = if relative.is_empty() {
            Vec::new()
        } else {
            parse_path(relative)?
        };
        if let Some(leaf) = get_value_at_segments_mut(value, &segments) {
            *leaf = Value::String(SENSITIVE_PLAINTEXT_MASK.to_string());
        }
    }
    Ok(findings)
}

/// Map a full sensitive path onto a subtree rooted at `root_path`.
/// Returns `Some("")` when the sensitive path *is* the root; `None` when unrelated.
fn relative_sensitive_path<'a>(sensitive_path: &'a str, root_path: Option<&str>) -> Option<&'a str> {
    match root_path {
        None => Some(sensitive_path),
        Some(root) if sensitive_path == root => Some(""),
        Some(root) => {
            let prefix = format!("{}.", root);
            sensitive_path.strip_prefix(&prefix)
        }
    }
}

fn get_value_at_segments<'a>(
    value: &'a Value,
    segments: &[crate::index::PathSegment],
) -> Option<&'a Value> {
    use crate::index::PathSegment;
    if segments.is_empty() {
        return Some(value);
    }
    match &segments[0] {
        PathSegment::Field(name) => match value {
            Value::Struct(fields) => {
                for (fname, _, child) in fields {
                    if fname == name {
                        return get_value_at_segments(child, &segments[1..]);
                    }
                }
                None
            }
            Value::Map(entries) => {
                for (key, child) in entries {
                    if let Value::String(k) = key {
                        if k == name {
                            return get_value_at_segments(child, &segments[1..]);
                        }
                    }
                }
                None
            }
            _ => None,
        },
        PathSegment::Index(idx) => match value {
            Value::List(items) => items
                .get(*idx)
                .and_then(|child| get_value_at_segments(child, &segments[1..])),
            _ => None,
        },
        PathSegment::WildcardIndex => None,
    }
}

fn get_value_at_segments_mut<'a>(
    value: &'a mut Value,
    segments: &[crate::index::PathSegment],
) -> Option<&'a mut Value> {
    use crate::index::PathSegment;
    if segments.is_empty() {
        return Some(value);
    }
    match &segments[0] {
        PathSegment::Field(name) => {
            let name = name.clone();
            match value {
                Value::Struct(fields) => {
                    for (fname, _, child) in fields.iter_mut() {
                        if *fname == name {
                            return get_value_at_segments_mut(child, &segments[1..]);
                        }
                    }
                    None
                }
                Value::Map(entries) => {
                    for (key, child) in entries.iter_mut() {
                        if let Value::String(k) = key {
                            if *k == name {
                                return get_value_at_segments_mut(child, &segments[1..]);
                            }
                        }
                    }
                    None
                }
                _ => None,
            }
        }
        PathSegment::Index(idx) => {
            let idx = *idx;
            match value {
                Value::List(items) => items
                    .get_mut(idx)
                    .and_then(|child| get_value_at_segments_mut(child, &segments[1..])),
                _ => None,
            }
        }
        PathSegment::WildcardIndex => None,
    }
}

#[cfg(test)]
mod sensitive_schema_tests {
    use super::*;

    #[test]
    fn sensitive_paths_round_trip_msgpack() {
        let mut schema = Schema::new("Root".into());
        schema.add_type(
            "Root".into(),
            TypeDefinition::Struct(
                [(
                    "password".into(),
                    FieldDefinition {
                        field_type: TypeDefinition::String,
                        required: true,
                        default: None,
                        constraints: vec![],
                        documentation: Some("db password".into()),
                        ai_tag: None,
                    },
                )]
                .into_iter()
                .collect(),
            ),
        );
        schema.mark_sensitive("password");
        let bytes = schema.to_msgpack().unwrap();
        let decoded = Schema::from_msgpack(&bytes).unwrap();
        assert!(decoded.is_sensitive("password"));
        let agent = decoded.to_agent_safe();
        assert!(agent.sensitive_paths.contains(&"password".into()));
        assert!(agent
            .paths
            .iter()
            .any(|p| p.path == "password" && p.sensitive));
    }

    #[test]
    fn find_sensitive_plaintext_detects_cleartext() {
        let mut schema = Schema::new("Root".into());
        schema.add_type("Root".into(), TypeDefinition::Null);
        schema.mark_sensitive("secret");
        let value = Value::Struct(vec![("secret".into(), 0, Value::String("hunter2".into()))]);
        let findings = find_sensitive_plaintext(&schema, &value).unwrap();
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn redact_sensitive_plaintext_masks_cleartext() {
        let mut schema = Schema::new("Root".into());
        schema.add_type("Root".into(), TypeDefinition::Null);
        schema.mark_sensitive("secret");
        let mut value = Value::Struct(vec![("secret".into(), 0, Value::String("hunter2".into()))]);
        let findings = redact_sensitive_plaintext(&schema, &mut value).unwrap();
        assert_eq!(findings.len(), 1);
        match &value {
            Value::Struct(fields) => match &fields[0].2 {
                Value::String(s) => assert_eq!(s, SENSITIVE_PLAINTEXT_MASK),
                other => panic!("expected string mask, got {:?}", other),
            },
            other => panic!("expected struct, got {:?}", other),
        }
    }

    #[test]
    fn find_sensitive_plaintext_ignores_mask_placeholders() {
        let mut schema = Schema::new("Root".into());
        schema.add_type("Root".into(), TypeDefinition::Null);
        schema.mark_sensitive("password");
        let value = Value::Struct(vec![(
            "password".into(),
            0,
            Value::String("[PROTECTED]".into()),
        )]);
        let findings = find_sensitive_plaintext(&schema, &value).unwrap();
        assert!(findings.is_empty());
    }

    #[test]
    fn find_sensitive_plaintext_under_path_root() {
        let mut schema = Schema::new("Root".into());
        schema.add_type("Root".into(), TypeDefinition::Null);
        schema.mark_sensitive("database.password");
        let mut leaf = Value::String("plain".into());
        let findings =
            find_sensitive_plaintext_under(&schema, &leaf, Some("database.password")).unwrap();
        assert_eq!(findings.len(), 1);
        redact_sensitive_plaintext_under(&schema, &mut leaf, Some("database.password")).unwrap();
        assert_eq!(leaf, Value::String(SENSITIVE_PLAINTEXT_MASK.into()));
    }
}
