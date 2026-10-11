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
fn telemetry_request_body_is_dropped() {
    let spec = patched(
        json!({"paths": {"/api/client-diagnostics/telemetry": {"post": {
        "requestBody": {"content": {"application/json": {}}}, "responses": {}}}}}),
    );
    assert_eq!(
        spec["paths"]["/api/client-diagnostics/telemetry"]["post"],
        json!({"responses": {}})
    );
}

#[test]
fn error_responses_are_dropped() {
    let spec = patched(json!({"paths": {"/api/jobs": {"post": {"responses": {
        "201": {}, "302": {}, "400": {}, "500": {}, "default": {}}}}}}));
    assert_eq!(
        spec["paths"]["/api/jobs"]["post"]["responses"],
        json!({"201": {}, "302": {}, "default": {}})
    );
}

#[test]
fn job_fields_the_server_leaves_out_are_optional() {
    let spec = patched(json!({"components": {"schemas": {"UserJobDto": {
        "required": ["id", "finishedAt", "startedAt", "result", "status"]}}}}));
    assert_eq!(
        spec["components"]["schemas"]["UserJobDto"]["required"],
        json!(["id", "status"])
    );
}
