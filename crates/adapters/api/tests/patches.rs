#[path = "../src/spec_patch.rs"]
mod spec_patch;

use serde_json::{Value, json};

fn patched(spec: Value) -> Value {
    let mut spec = spec;
    spec_patch::apply(&mut spec);
    spec
}

#[test]
fn multipart_operations_are_dropped() {
    let spec = patched(json!({"paths": {"/api/image": {
        "post": {"requestBody": {"content": {"multipart/form-data": {}}}},
        "get": {"responses": {}}
    }}}));
    assert_eq!(
        spec["paths"]["/api/image"],
        json!({"get": {"responses": {}}})
    );
}

#[test]
fn format_without_type_becomes_a_string() {
    let spec = patched(json!({"components": {"schemas": {"Id": {"format": "uuid"},
        "Props": {"properties": {"format": {"type": "string"}}}}}}));
    assert_eq!(spec["components"]["schemas"]["Id"]["type"], "string");
    assert!(spec["components"]["schemas"]["Props"].get("type").is_none());
}

#[test]
fn closed_objects_accept_unknown_fields() {
    let spec = patched(json!({"components": {"schemas": {"Job": {"type": "object",
        "additionalProperties": false, "properties": {}}}}}));
    assert!(
        spec["components"]["schemas"]["Job"]
            .get("additionalProperties")
            .is_none()
    );
}

#[test]
fn string_enums_are_opened_except_discriminators() {
    let spec = patched(json!({"components": {"schemas": {
        "Status": {"type": "string", "enum": ["NEW", "FAILED"]},
        "Body": {"type": "object", "properties": {"code": {"type": "string", "enum": ["ONLY"]},
            "errors": {"type": "array", "items": {"oneOf": [{"$ref": "#/components/schemas/Branch"}]}}}},
        "Branch": {"type": "object", "properties": {"code": {"type": "string", "enum": ["TOO_LARGE"]}}}
    }}}));
    let schemas = &spec["components"]["schemas"];
    let open = json!([{"type": "string", "enum": ["NEW", "FAILED"]}, {"type": "string"}]);
    assert_eq!(schemas["Status"]["oneOf"], open);
    assert_eq!(
        schemas["Body"]["properties"]["code"]["oneOf"][1],
        json!({"type": "string"})
    );
    assert_eq!(
        schemas["Branch"]["properties"]["code"],
        json!({"type": "string", "enum": ["TOO_LARGE"]})
    );
}

#[test]
fn object_unions_get_a_catch_all_branch() {
    let spec = patched(json!({"components": {"schemas": {"Item": {
        "discriminator": {"propertyName": "kind"},
        "oneOf": [{"type": "object", "properties": {}}, {"$ref": "#/components/schemas/Other"}]
    }}}}));
    let item = &spec["components"]["schemas"]["Item"];
    assert_eq!(item["oneOf"][1], json!({}));
    assert_eq!(
        item["oneOf"][0]["discriminator"],
        json!({"propertyName": "kind"})
    );
    assert!(item.get("discriminator").is_none());
}

#[test]
fn deprecated_elements_are_dropped() {
    let spec = patched(json!({
        "paths": {"/api/old": {"get": {"deprecated": true}, "post": {"parameters": [
            {"name": "keep"}, {"name": "gone", "deprecated": true}]}}},
        "components": {"schemas": {"Image": {"type": "object", "required": ["id", "validated"],
            "properties": {"id": {"type": "string"},
                "validated": {"type": "boolean", "deprecated": true}}}}}
    }));
    assert!(spec["paths"]["/api/old"].get("get").is_none());
    assert_eq!(
        spec["paths"]["/api/old"]["post"]["parameters"],
        json!([{"name": "keep"}])
    );
    let image = &spec["components"]["schemas"]["Image"];
    assert_eq!(image["required"], json!(["id"]));
    assert!(image["properties"].get("validated").is_none());
}

#[test]
fn references_to_primitive_named_components_become_primitives() {
    let spec = patched(json!({"components": {"schemas": {"Job": {"properties": {
        "archived": {"allOf": [{"$ref": "#/components/schemas/boolean"}]}}}}}}));
    let archived = &spec["components"]["schemas"]["Job"]["properties"]["archived"];
    assert_eq!(archived["allOf"][0], json!({"type": "boolean"}));
}
