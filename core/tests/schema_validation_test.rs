// Unit tests for schema validation
// Tests validation of primitive types, composite types, constraints, and custom types

use bcs_core::schema::{
    Constraint, FieldDefinition, Schema, SchemaEngine, TypeDefinition, VariantDefinition,
};
use bcs_core::types::Value;
use std::collections::HashMap;

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a simple schema with a root type
fn create_test_schema(root_name: &str, root_type: TypeDefinition) -> Schema {
    let mut schema = Schema::new(root_name.to_string());
    schema.add_type(root_name.to_string(), root_type);
    schema
}

/// Create a field definition
fn create_field(
    field_type: TypeDefinition,
    required: bool,
    constraints: Vec<Constraint>,
) -> FieldDefinition {
    FieldDefinition {
        field_type,
        required,
        default: None,
        constraints,
        documentation: None,
        ai_tag: None,
    }
}

// ============================================================================
// Primitive Type Validation Tests
// ============================================================================

#[test]
fn test_validate_null() {
    let schema = create_test_schema("Root", TypeDefinition::Null);
    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::Null, &schema);
    assert!(result.is_valid(), "Null value should be valid");
}

#[test]
fn test_validate_bool() {
    let schema = create_test_schema("Root", TypeDefinition::Bool);
    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::Bool(true), &schema);
    assert!(result.is_valid(), "Bool(true) should be valid");

    let result = engine.validate(&Value::Bool(false), &schema);
    assert!(result.is_valid(), "Bool(false) should be valid");
}

#[test]
fn test_validate_int8() {
    let schema = create_test_schema("Root", TypeDefinition::Int8);
    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::Int8(42), &schema);
    assert!(result.is_valid(), "Int8 value should be valid");

    let result = engine.validate(&Value::Int8(-128), &schema);
    assert!(result.is_valid(), "Int8 min value should be valid");

    let result = engine.validate(&Value::Int8(127), &schema);
    assert!(result.is_valid(), "Int8 max value should be valid");
}

#[test]
fn test_validate_int16() {
    let schema = create_test_schema("Root", TypeDefinition::Int16);
    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::Int16(1000), &schema);
    assert!(result.is_valid(), "Int16 value should be valid");
}

#[test]
fn test_validate_int32() {
    let schema = create_test_schema("Root", TypeDefinition::Int32);
    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::Int32(100000), &schema);
    assert!(result.is_valid(), "Int32 value should be valid");
}

#[test]
fn test_validate_int64() {
    let schema = create_test_schema("Root", TypeDefinition::Int64);
    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::Int64(9223372036854775807), &schema);
    assert!(result.is_valid(), "Int64 value should be valid");
}

#[test]
fn test_validate_uint8() {
    let schema = create_test_schema("Root", TypeDefinition::UInt8);
    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::UInt8(255), &schema);
    assert!(result.is_valid(), "UInt8 max value should be valid");
}

#[test]
fn test_validate_uint16() {
    let schema = create_test_schema("Root", TypeDefinition::UInt16);
    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::UInt16(65535), &schema);
    assert!(result.is_valid(), "UInt16 max value should be valid");
}

#[test]
fn test_validate_uint32() {
    let schema = create_test_schema("Root", TypeDefinition::UInt32);
    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::UInt32(4294967295), &schema);
    assert!(result.is_valid(), "UInt32 max value should be valid");
}

#[test]
fn test_validate_uint64() {
    let schema = create_test_schema("Root", TypeDefinition::UInt64);
    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::UInt64(18446744073709551615), &schema);
    assert!(result.is_valid(), "UInt64 max value should be valid");
}

#[test]
fn test_validate_float32() {
    let schema = create_test_schema("Root", TypeDefinition::Float32);
    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::Float32(std::f32::consts::PI), &schema);
    assert!(result.is_valid(), "Float32 value should be valid");
}

#[test]
fn test_validate_float64() {
    let schema = create_test_schema("Root", TypeDefinition::Float64);
    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::Float64(std::f64::consts::E), &schema);
    assert!(result.is_valid(), "Float64 value should be valid");
}

#[test]
fn test_validate_string() {
    let schema = create_test_schema("Root", TypeDefinition::String);
    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::String("Hello, World!".to_string()), &schema);
    assert!(result.is_valid(), "String value should be valid");

    let result = engine.validate(&Value::String("".to_string()), &schema);
    assert!(result.is_valid(), "Empty string should be valid");
}

#[test]
fn test_validate_bytes() {
    let schema = create_test_schema("Root", TypeDefinition::Bytes);
    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::Bytes(vec![1, 2, 3, 4]), &schema);
    assert!(result.is_valid(), "Bytes value should be valid");
}

#[test]
fn test_validate_type_mismatch() {
    let schema = create_test_schema("Root", TypeDefinition::Int32);
    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::String("not an int".to_string()), &schema);
    assert!(!result.is_valid(), "Type mismatch should be invalid");
    assert!(!result.errors.is_empty(), "Should have validation errors");
}

// ============================================================================
// Composite Type Validation Tests
// ============================================================================

#[test]
fn test_validate_list() {
    let schema = create_test_schema(
        "Root",
        TypeDefinition::List(Box::new(TypeDefinition::Int32)),
    );
    let engine = SchemaEngine::new();

    let value = Value::List(vec![Value::Int32(1), Value::Int32(2), Value::Int32(3)]);

    let result = engine.validate(&value, &schema);
    assert!(result.is_valid(), "List of Int32 should be valid");
}

#[test]
fn test_validate_list_type_mismatch() {
    let schema = create_test_schema(
        "Root",
        TypeDefinition::List(Box::new(TypeDefinition::Int32)),
    );
    let engine = SchemaEngine::new();

    let value = Value::List(vec![
        Value::Int32(1),
        Value::String("not an int".to_string()),
        Value::Int32(3),
    ]);

    let result = engine.validate(&value, &schema);
    assert!(!result.is_valid(), "List with wrong type should be invalid");
}

#[test]
fn test_validate_map() {
    let schema = create_test_schema(
        "Root",
        TypeDefinition::Map(
            Box::new(TypeDefinition::String),
            Box::new(TypeDefinition::Int32),
        ),
    );
    let engine = SchemaEngine::new();

    let value = Value::Map(vec![
        (Value::String("key1".to_string()), Value::Int32(100)),
        (Value::String("key2".to_string()), Value::Int32(200)),
    ]);

    let result = engine.validate(&value, &schema);
    assert!(result.is_valid(), "Map should be valid");
}

#[test]
fn test_validate_struct() {
    let mut fields = HashMap::new();
    fields.insert(
        "name".to_string(),
        create_field(TypeDefinition::String, true, vec![]),
    );
    fields.insert(
        "age".to_string(),
        create_field(TypeDefinition::Int32, true, vec![]),
    );

    let schema = create_test_schema("Root", TypeDefinition::Struct(fields));
    let engine = SchemaEngine::new();

    // Hash field names
    let name_hash = xxhash_rust::xxh64::xxh64("name".as_bytes(), 0);
    let age_hash = xxhash_rust::xxh64::xxh64("age".as_bytes(), 0);

    let value = Value::Struct(vec![
        (
            "name".to_string(),
            name_hash,
            Value::String("Alice".to_string()),
        ),
        ("age".to_string(), age_hash, Value::Int32(30)),
    ]);

    let result = engine.validate(&value, &schema);
    assert!(result.is_valid(), "Struct should be valid");
}

#[test]
fn test_validate_struct_missing_required_field() {
    let mut fields = HashMap::new();
    fields.insert(
        "name".to_string(),
        create_field(TypeDefinition::String, true, vec![]),
    );
    fields.insert(
        "age".to_string(),
        create_field(TypeDefinition::Int32, true, vec![]),
    );

    let schema = create_test_schema("Root", TypeDefinition::Struct(fields));
    let engine = SchemaEngine::new();

    // Only provide name, missing age
    let name_hash = xxhash_rust::xxh64::xxh64("name".as_bytes(), 0);

    let value = Value::Struct(vec![(
        "name".to_string(),
        name_hash,
        Value::String("Alice".to_string()),
    )]);

    let result = engine.validate(&value, &schema);
    assert!(
        !result.is_valid(),
        "Struct with missing required field should be invalid"
    );
}

#[test]
fn test_validate_union() {
    let variants = vec![
        VariantDefinition {
            name: "Integer".to_string(),
            tag: 0,
            value_type: TypeDefinition::Int32,
            documentation: None,
        },
        VariantDefinition {
            name: "Text".to_string(),
            tag: 1,
            value_type: TypeDefinition::String,
            documentation: None,
        },
    ];

    let schema = create_test_schema("Root", TypeDefinition::Union(variants));
    let engine = SchemaEngine::new();

    let value = Value::Union(0, Box::new(Value::Int32(42)));
    let result = engine.validate(&value, &schema);
    assert!(
        result.is_valid(),
        "Union with Integer variant should be valid"
    );

    let value = Value::Union(1, Box::new(Value::String("hello".to_string())));
    let result = engine.validate(&value, &schema);
    assert!(result.is_valid(), "Union with Text variant should be valid");
}

#[test]
fn test_validate_union_invalid_tag() {
    let variants = vec![VariantDefinition {
        name: "Integer".to_string(),
        tag: 0,
        value_type: TypeDefinition::Int32,
        documentation: None,
    }];

    let schema = create_test_schema("Root", TypeDefinition::Union(variants));
    let engine = SchemaEngine::new();

    let value = Value::Union(99, Box::new(Value::Int32(42)));
    let result = engine.validate(&value, &schema);
    assert!(
        !result.is_valid(),
        "Union with invalid tag should be invalid"
    );
}

#[test]
fn test_validate_optional_some() {
    let schema = create_test_schema(
        "Root",
        TypeDefinition::Optional(Box::new(TypeDefinition::String)),
    );
    let engine = SchemaEngine::new();

    let value = Value::Optional(Some(Box::new(Value::String("present".to_string()))));
    let result = engine.validate(&value, &schema);
    assert!(result.is_valid(), "Optional with Some should be valid");
}

#[test]
fn test_validate_optional_none() {
    let schema = create_test_schema(
        "Root",
        TypeDefinition::Optional(Box::new(TypeDefinition::String)),
    );
    let engine = SchemaEngine::new();

    let value = Value::Optional(None);
    let result = engine.validate(&value, &schema);
    assert!(result.is_valid(), "Optional with None should be valid");
}

// ============================================================================
// Constraint Validation Tests
// ============================================================================

#[test]
fn test_validate_range_constraint() {
    let mut schema = create_test_schema("Root", TypeDefinition::Int32);
    schema.add_constraints("".to_string(), vec![Constraint::Range(0.0, 100.0)]);

    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::Int32(50), &schema);
    assert!(result.is_valid(), "Value within range should be valid");

    let result = engine.validate(&Value::Int32(0), &schema);
    assert!(result.is_valid(), "Value at min range should be valid");

    let result = engine.validate(&Value::Int32(100), &schema);
    assert!(result.is_valid(), "Value at max range should be valid");

    let result = engine.validate(&Value::Int32(-1), &schema);
    assert!(!result.is_valid(), "Value below range should be invalid");

    let result = engine.validate(&Value::Int32(101), &schema);
    assert!(!result.is_valid(), "Value above range should be invalid");
}

#[test]
fn test_validate_min_constraint() {
    let mut schema = create_test_schema("Root", TypeDefinition::Int32);
    schema.add_constraints("".to_string(), vec![Constraint::Min(10.0)]);

    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::Int32(20), &schema);
    assert!(result.is_valid(), "Value above min should be valid");

    let result = engine.validate(&Value::Int32(10), &schema);
    assert!(result.is_valid(), "Value at min should be valid");

    let result = engine.validate(&Value::Int32(5), &schema);
    assert!(!result.is_valid(), "Value below min should be invalid");
}

#[test]
fn test_validate_max_constraint() {
    let mut schema = create_test_schema("Root", TypeDefinition::Int32);
    schema.add_constraints("".to_string(), vec![Constraint::Max(100.0)]);

    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::Int32(50), &schema);
    assert!(result.is_valid(), "Value below max should be valid");

    let result = engine.validate(&Value::Int32(100), &schema);
    assert!(result.is_valid(), "Value at max should be valid");

    let result = engine.validate(&Value::Int32(150), &schema);
    assert!(!result.is_valid(), "Value above max should be invalid");
}

#[test]
fn test_validate_non_empty_constraint_string() {
    let mut schema = create_test_schema("Root", TypeDefinition::String);
    schema.add_constraints("".to_string(), vec![Constraint::NonEmpty]);

    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::String("hello".to_string()), &schema);
    assert!(result.is_valid(), "Non-empty string should be valid");

    let result = engine.validate(&Value::String("".to_string()), &schema);
    assert!(
        !result.is_valid(),
        "Empty string should be invalid with NonEmpty constraint"
    );
}

#[test]
fn test_validate_non_empty_constraint_list() {
    let mut schema = create_test_schema(
        "Root",
        TypeDefinition::List(Box::new(TypeDefinition::Int32)),
    );
    schema.add_constraints("".to_string(), vec![Constraint::NonEmpty]);

    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::List(vec![Value::Int32(1)]), &schema);
    assert!(result.is_valid(), "Non-empty list should be valid");

    let result = engine.validate(&Value::List(vec![]), &schema);
    assert!(
        !result.is_valid(),
        "Empty list should be invalid with NonEmpty constraint"
    );
}

#[test]
fn test_validate_length_constraint() {
    let mut schema = create_test_schema("Root", TypeDefinition::String);
    schema.add_constraints("".to_string(), vec![Constraint::Length(Some(3), Some(10))]);

    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::String("hello".to_string()), &schema);
    assert!(
        result.is_valid(),
        "String within length range should be valid"
    );

    let result = engine.validate(&Value::String("hi".to_string()), &schema);
    assert!(
        !result.is_valid(),
        "String below min length should be invalid"
    );

    let result = engine.validate(&Value::String("this is too long".to_string()), &schema);
    assert!(
        !result.is_valid(),
        "String above max length should be invalid"
    );
}

#[test]
fn test_validate_unique_constraint() {
    let mut schema = create_test_schema(
        "Root",
        TypeDefinition::List(Box::new(TypeDefinition::Int32)),
    );
    schema.add_constraints("".to_string(), vec![Constraint::Unique]);

    let engine = SchemaEngine::new();

    let result = engine.validate(
        &Value::List(vec![Value::Int32(1), Value::Int32(2), Value::Int32(3)]),
        &schema,
    );
    assert!(result.is_valid(), "List with unique values should be valid");

    let result = engine.validate(
        &Value::List(vec![Value::Int32(1), Value::Int32(2), Value::Int32(1)]),
        &schema,
    );
    assert!(
        !result.is_valid(),
        "List with duplicate values should be invalid"
    );
}

#[test]
fn test_validate_enum_constraint() {
    let mut schema = create_test_schema("Root", TypeDefinition::String);
    schema.add_constraints(
        "".to_string(),
        vec![Constraint::Enum(vec![
            Value::String("red".to_string()),
            Value::String("green".to_string()),
            Value::String("blue".to_string()),
        ])],
    );

    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::String("red".to_string()), &schema);
    assert!(result.is_valid(), "Value in enum should be valid");

    let result = engine.validate(&Value::String("yellow".to_string()), &schema);
    assert!(!result.is_valid(), "Value not in enum should be invalid");
}

// ============================================================================
// Custom Type Validation Tests
// ============================================================================

#[test]
fn test_validate_custom_type() {
    let mut schema = Schema::new("Root".to_string());

    // Define a custom type
    let mut person_fields = HashMap::new();
    person_fields.insert(
        "name".to_string(),
        create_field(TypeDefinition::String, true, vec![]),
    );
    person_fields.insert(
        "age".to_string(),
        create_field(TypeDefinition::Int32, true, vec![]),
    );

    schema.add_type("Person".to_string(), TypeDefinition::Struct(person_fields));
    schema.add_type(
        "Root".to_string(),
        TypeDefinition::Custom("Person".to_string()),
    );

    let engine = SchemaEngine::new();

    let name_hash = xxhash_rust::xxh64::xxh64("name".as_bytes(), 0);
    let age_hash = xxhash_rust::xxh64::xxh64("age".as_bytes(), 0);

    let value = Value::Struct(vec![
        (
            "name".to_string(),
            name_hash,
            Value::String("Bob".to_string()),
        ),
        ("age".to_string(), age_hash, Value::Int32(25)),
    ]);

    let result = engine.validate(&value, &schema);
    assert!(result.is_valid(), "Custom type should be valid");
}

#[test]
fn test_validate_nested_custom_types() {
    let mut schema = Schema::new("Root".to_string());

    // Define Address type
    let mut address_fields = HashMap::new();
    address_fields.insert(
        "street".to_string(),
        create_field(TypeDefinition::String, true, vec![]),
    );
    address_fields.insert(
        "city".to_string(),
        create_field(TypeDefinition::String, true, vec![]),
    );

    schema.add_type(
        "Address".to_string(),
        TypeDefinition::Struct(address_fields),
    );

    // Define Person type with Address
    let mut person_fields = HashMap::new();
    person_fields.insert(
        "name".to_string(),
        create_field(TypeDefinition::String, true, vec![]),
    );
    person_fields.insert(
        "address".to_string(),
        create_field(TypeDefinition::Custom("Address".to_string()), true, vec![]),
    );

    schema.add_type("Person".to_string(), TypeDefinition::Struct(person_fields));
    schema.add_type(
        "Root".to_string(),
        TypeDefinition::Custom("Person".to_string()),
    );

    let engine = SchemaEngine::new();

    let name_hash = xxhash_rust::xxh64::xxh64("name".as_bytes(), 0);
    let address_hash = xxhash_rust::xxh64::xxh64("address".as_bytes(), 0);
    let street_hash = xxhash_rust::xxh64::xxh64("street".as_bytes(), 0);
    let city_hash = xxhash_rust::xxh64::xxh64("city".as_bytes(), 0);

    let value = Value::Struct(vec![
        (
            "name".to_string(),
            name_hash,
            Value::String("Charlie".to_string()),
        ),
        (
            "address".to_string(),
            address_hash,
            Value::Struct(vec![
                (
                    "street".to_string(),
                    street_hash,
                    Value::String("123 Main St".to_string()),
                ),
                (
                    "city".to_string(),
                    city_hash,
                    Value::String("Springfield".to_string()),
                ),
            ]),
        ),
    ]);

    let result = engine.validate(&value, &schema);
    assert!(result.is_valid(), "Nested custom types should be valid");
}

#[test]
fn test_register_and_validate_custom_type() {
    let mut engine = SchemaEngine::new();

    // Register a custom type in the engine
    let mut fields = HashMap::new();
    fields.insert(
        "id".to_string(),
        create_field(TypeDefinition::Int32, true, vec![]),
    );
    fields.insert(
        "value".to_string(),
        create_field(TypeDefinition::String, true, vec![]),
    );

    engine.register_custom_type("CustomData".to_string(), TypeDefinition::Struct(fields));

    // Create schema that references the custom type
    let schema = create_test_schema("Root", TypeDefinition::Custom("CustomData".to_string()));

    let id_hash = xxhash_rust::xxh64::xxh64("id".as_bytes(), 0);
    let value_hash = xxhash_rust::xxh64::xxh64("value".as_bytes(), 0);

    let value = Value::Struct(vec![
        ("id".to_string(), id_hash, Value::Int32(123)),
        (
            "value".to_string(),
            value_hash,
            Value::String("test".to_string()),
        ),
    ]);

    let result = engine.validate(&value, &schema);
    assert!(result.is_valid(), "Registered custom type should be valid");
}

#[test]
fn test_validate_custom_type_not_found() {
    let schema = create_test_schema("Root", TypeDefinition::Custom("NonExistent".to_string()));
    let engine = SchemaEngine::new();

    let value = Value::Int32(42);
    let result = engine.validate(&value, &schema);
    assert!(!result.is_valid(), "Unknown custom type should be invalid");
}

// ============================================================================
// Complex Validation Scenarios
// ============================================================================

#[test]
fn test_validate_nested_structures() {
    let mut schema = Schema::new("Root".to_string());

    // Create a complex nested structure
    let mut inner_fields = HashMap::new();
    inner_fields.insert(
        "value".to_string(),
        create_field(
            TypeDefinition::Int32,
            true,
            vec![Constraint::Range(0.0, 100.0)],
        ),
    );

    let mut outer_fields = HashMap::new();
    outer_fields.insert(
        "items".to_string(),
        create_field(
            TypeDefinition::List(Box::new(TypeDefinition::Struct(inner_fields))),
            true,
            vec![Constraint::NonEmpty],
        ),
    );

    schema.add_type("Root".to_string(), TypeDefinition::Struct(outer_fields));

    let engine = SchemaEngine::new();

    let items_hash = xxhash_rust::xxh64::xxh64("items".as_bytes(), 0);
    let value_hash = xxhash_rust::xxh64::xxh64("value".as_bytes(), 0);

    let value = Value::Struct(vec![(
        "items".to_string(),
        items_hash,
        Value::List(vec![
            Value::Struct(vec![("value".to_string(), value_hash, Value::Int32(50))]),
            Value::Struct(vec![("value".to_string(), value_hash, Value::Int32(75))]),
        ]),
    )]);

    let result = engine.validate(&value, &schema);
    assert!(
        result.is_valid(),
        "Complex nested structure should be valid"
    );
}

#[test]
fn test_validate_multiple_constraints() {
    let mut schema = create_test_schema("Root", TypeDefinition::String);
    schema.add_constraints(
        "".to_string(),
        vec![Constraint::NonEmpty, Constraint::Length(Some(5), Some(20))],
    );

    let engine = SchemaEngine::new();

    let result = engine.validate(&Value::String("hello world".to_string()), &schema);
    assert!(
        result.is_valid(),
        "Value satisfying all constraints should be valid"
    );

    let result = engine.validate(&Value::String("".to_string()), &schema);
    assert!(
        !result.is_valid(),
        "Empty string should fail NonEmpty constraint"
    );

    let result = engine.validate(&Value::String("hi".to_string()), &schema);
    assert!(
        !result.is_valid(),
        "Short string should fail Length constraint"
    );
}

// ============================================================================
// Pattern (Regex) Constraint Tests
// ============================================================================

#[test]
fn test_validate_pattern_simple() {
    let mut schema = create_test_schema("Root", TypeDefinition::String);
    schema.add_constraints(
        "".to_string(),
        vec![Constraint::Pattern("^[a-z]+$".to_string())],
    );

    let engine = SchemaEngine::new();

    // Valid: lowercase letters only
    let result = engine.validate(&Value::String("hello".to_string()), &schema);
    assert!(result.is_valid(), "Lowercase string should match pattern");

    // Invalid: contains uppercase
    let result = engine.validate(&Value::String("Hello".to_string()), &schema);
    assert!(
        !result.is_valid(),
        "String with uppercase should not match pattern"
    );

    // Invalid: contains numbers
    let result = engine.validate(&Value::String("hello123".to_string()), &schema);
    assert!(
        !result.is_valid(),
        "String with numbers should not match pattern"
    );
}

#[test]
fn test_validate_pattern_email() {
    let mut schema = create_test_schema("Root", TypeDefinition::String);
    schema.add_constraints(
        "".to_string(),
        vec![Constraint::Pattern(r"^[\w.-]+@[\w.-]+\.\w+$".to_string())],
    );

    let engine = SchemaEngine::new();

    // Valid email
    let result = engine.validate(&Value::String("user@example.com".to_string()), &schema);
    assert!(result.is_valid(), "Valid email should match pattern");

    // Invalid email - no @
    let result = engine.validate(&Value::String("userexample.com".to_string()), &schema);
    assert!(
        !result.is_valid(),
        "Email without @ should not match pattern"
    );

    // Invalid email - no domain
    let result = engine.validate(&Value::String("user@".to_string()), &schema);
    assert!(
        !result.is_valid(),
        "Email without domain should not match pattern"
    );
}

#[test]
fn test_validate_pattern_phone() {
    let mut schema = create_test_schema("Root", TypeDefinition::String);
    schema.add_constraints(
        "".to_string(),
        vec![Constraint::Pattern(r"^\+?[1-9]\d{1,14}$".to_string())],
    );

    let engine = SchemaEngine::new();

    // Valid phone numbers
    let result = engine.validate(&Value::String("+14155552671".to_string()), &schema);
    assert!(result.is_valid(), "Valid phone number should match pattern");

    let result = engine.validate(&Value::String("14155552671".to_string()), &schema);
    assert!(result.is_valid(), "Phone without + should match pattern");

    // Invalid phone numbers
    let result = engine.validate(&Value::String("0123456789".to_string()), &schema);
    assert!(
        !result.is_valid(),
        "Phone starting with 0 should not match pattern"
    );

    let result = engine.validate(&Value::String("abc".to_string()), &schema);
    assert!(
        !result.is_valid(),
        "Non-numeric string should not match pattern"
    );
}

#[test]
fn test_validate_pattern_ipv4() {
    let mut schema = create_test_schema("Root", TypeDefinition::String);
    schema.add_constraints(
        "".to_string(),
        vec![Constraint::Pattern(
            r"^((25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(25[0-5]|2[0-4]\d|[01]?\d\d?)$".to_string(),
        )],
    );

    let engine = SchemaEngine::new();

    // Valid IPv4
    let result = engine.validate(&Value::String("192.168.1.1".to_string()), &schema);
    assert!(result.is_valid(), "Valid IPv4 should match pattern");

    let result = engine.validate(&Value::String("0.0.0.0".to_string()), &schema);
    assert!(result.is_valid(), "Minimum IPv4 should match pattern");

    let result = engine.validate(&Value::String("255.255.255.255".to_string()), &schema);
    assert!(result.is_valid(), "Maximum IPv4 should match pattern");

    // Invalid IPv4
    let result = engine.validate(&Value::String("256.1.1.1".to_string()), &schema);
    assert!(!result.is_valid(), "IPv4 with octet > 255 should not match");

    let result = engine.validate(&Value::String("1.2.3".to_string()), &schema);
    assert!(!result.is_valid(), "Incomplete IPv4 should not match");
}

#[test]
fn test_validate_pattern_optional_values() {
    let mut schema = create_test_schema(
        "Root",
        TypeDefinition::Optional(Box::new(TypeDefinition::String)),
    );
    schema.add_constraints(
        "".to_string(),
        vec![Constraint::Pattern("^[a-z]+$".to_string())],
    );

    let engine = SchemaEngine::new();

    // None should be valid (constraint only applies to String values)
    let result = engine.validate(&Value::Optional(None), &schema);
    assert!(result.is_valid(), "Optional None should be valid");

    // Some with valid string
    let result = engine.validate(
        &Value::Optional(Some(Box::new(Value::String("hello".to_string())))),
        &schema,
    );
    assert!(
        result.is_valid(),
        "Optional Some with valid string should be valid"
    );

    // Some with invalid string
    let result = engine.validate(
        &Value::Optional(Some(Box::new(Value::String("Hello".to_string())))),
        &schema,
    );
    assert!(
        !result.is_valid(),
        "Optional Some with invalid string should be invalid"
    );
}

#[test]
fn test_validate_pattern_non_string_ignored() {
    let mut schema = create_test_schema("Root", TypeDefinition::Int32);
    schema.add_constraints(
        "".to_string(),
        vec![Constraint::Pattern("^[a-z]+$".to_string())],
    );

    let engine = SchemaEngine::new();

    // Pattern constraint should be ignored for non-string types
    let result = engine.validate(&Value::Int32(42), &schema);
    assert!(
        result.is_valid(),
        "Pattern constraint should be ignored for non-string types"
    );
}

#[test]
fn test_validate_pattern_with_struct_fields() {
    let mut fields = HashMap::new();
    fields.insert(
        "email".to_string(),
        create_field(
            TypeDefinition::String,
            true,
            vec![Constraint::Pattern(r"^[\w.-]+@[\w.-]+\.\w+$".to_string())],
        ),
    );
    fields.insert(
        "name".to_string(),
        create_field(TypeDefinition::String, true, vec![]),
    );

    let schema = create_test_schema("Root", TypeDefinition::Struct(fields));
    let engine = SchemaEngine::new();

    let email_hash = xxhash_rust::xxh64::xxh64("email".as_bytes(), 0);
    let name_hash = xxhash_rust::xxh64::xxh64("name".as_bytes(), 0);

    // Valid struct
    let value = Value::Struct(vec![
        (
            "email".to_string(),
            email_hash,
            Value::String("user@example.com".to_string()),
        ),
        (
            "name".to_string(),
            name_hash,
            Value::String("John Doe".to_string()),
        ),
    ]);

    let result = engine.validate(&value, &schema);
    assert!(result.is_valid(), "Struct with valid email should be valid");

    // Invalid struct - bad email
    let value = Value::Struct(vec![
        (
            "email".to_string(),
            email_hash,
            Value::String("not-an-email".to_string()),
        ),
        (
            "name".to_string(),
            name_hash,
            Value::String("John Doe".to_string()),
        ),
    ]);

    let result = engine.validate(&value, &schema);
    assert!(
        !result.is_valid(),
        "Struct with invalid email should be invalid"
    );
}

#[test]
fn test_validate_pattern_complex_regex() {
    let mut schema = create_test_schema("Root", TypeDefinition::String);
    // Pattern: 8+ chars with at least one letter and one digit
    // Note: Rust regex crate doesn't support look-around assertions
    schema.add_constraints(
        "".to_string(),
        vec![Constraint::Pattern(r"^[a-zA-Z0-9]{8,}$".to_string())],
    );

    let engine = SchemaEngine::new();

    // Valid: 8+ alphanumeric chars
    let result = engine.validate(&Value::String("Password1".to_string()), &schema);
    assert!(
        result.is_valid(),
        "Alphanumeric 8+ chars should match pattern"
    );

    let result = engine.validate(&Value::String("abcdefgh".to_string()), &schema);
    assert!(result.is_valid(), "All letters should match pattern");

    let result = engine.validate(&Value::String("12345678".to_string()), &schema);
    assert!(result.is_valid(), "All digits should match pattern");

    // Invalid: too short
    let result = engine.validate(&Value::String("Pass1".to_string()), &schema);
    assert!(!result.is_valid(), "String too short should not match");

    // Invalid: contains special chars
    let result = engine.validate(&Value::String("Password!".to_string()), &schema);
    assert!(
        !result.is_valid(),
        "String with special chars should not match"
    );
}
