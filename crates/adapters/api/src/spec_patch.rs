use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

type Schema = Map<String, Value>;

const MULTIPART: &str = "/requestBody/content/multipart~1form-data";
const TELEMETRY: &str = "/paths/~1api~1client-diagnostics~1telemetry/post";
const NOT_SENT: &[(&str, &[&str])] = &[("UserJobDto", &["finishedAt", "startedAt", "result"])];

pub(crate) fn apply(spec: &mut Value) {
    drop_multipart_operations(spec);
    drop_telemetry_body(spec);
    drop_error_responses(spec);
    relax_required(spec);
    declare_job_result_id(spec);
    walk(spec, &mut allow_unknown_fields);
    mark_discriminators(spec);
    walk(spec, &mut open_string_enum);
    walk(spec, &mut open_object_union);
    drop_deprecated(spec);
}

fn walk(node: &mut Value, patch: &mut dyn FnMut(&mut Schema)) {
    match node {
        Value::Object(map) => {
            map.values_mut().for_each(|child| walk(child, patch));
            patch(map);
        }
        Value::Array(items) => items.iter_mut().for_each(|child| walk(child, patch)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn operations(spec: &mut Value) -> impl Iterator<Item = &mut Map<String, Value>> {
    spec.get_mut("paths")
        .and_then(Value::as_object_mut)
        .into_iter()
        .flat_map(|paths| paths.values_mut())
        .filter_map(Value::as_object_mut)
}

fn drop_multipart_operations(spec: &mut Value) {
    for item in operations(spec) {
        item.retain(|_, operation| operation.pointer(MULTIPART).is_none());
    }
}

fn drop_error_responses(spec: &mut Value) {
    for item in operations(spec) {
        let responses = item
            .values_mut()
            .filter_map(|operation| operation.get_mut("responses"))
            .filter_map(Value::as_object_mut);
        for responses in responses {
            responses.retain(|status, _| !status.starts_with(['4', '5']));
        }
    }
}

fn relax_required(spec: &mut Value) {
    for (schema, fields) in NOT_SENT {
        let pointer = format!("/components/schemas/{schema}/required");
        if let Some(required) = spec.pointer_mut(&pointer).and_then(Value::as_array_mut) {
            required.retain(|name| !name.as_str().is_some_and(|name| fields.contains(&name)));
        }
    }
}

fn declare_job_result_id(spec: &mut Value) {
    let pointer = "/components/schemas/UserJobDto/properties";
    if let Some(properties) = spec.pointer_mut(pointer).and_then(Value::as_object_mut) {
        properties.insert(
            "resultId".to_owned(),
            json!({"type": "string", "format": "uuid"}),
        );
    }
}

fn drop_telemetry_body(spec: &mut Value) {
    if let Some(operation) = spec.pointer_mut(TELEMETRY).and_then(Value::as_object_mut) {
        operation.remove("requestBody");
    }
}

fn allow_unknown_fields(schema: &mut Map<String, Value>) {
    if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
        schema.remove("additionalProperties");
    }
}

const CLOSED: &str = "x-mivia-closed";

fn mark_discriminators(spec: &mut Value) {
    let mut members = BTreeSet::new();
    collect_union_refs(spec, &mut members);
    let components = spec
        .pointer_mut("/components/schemas")
        .and_then(Value::as_object_mut);
    for (name, schema) in components.into_iter().flatten() {
        if members.contains(&format!("#/components/schemas/{name}")) {
            mark_single_values(schema);
        }
    }
    walk(spec, &mut |schema| {
        let branches = schema.get_mut("oneOf").and_then(Value::as_array_mut);
        branches.into_iter().flatten().for_each(mark_single_values);
    });
}

fn mark_single_values(schema: &mut Value) {
    let properties = schema.get_mut("properties").and_then(Value::as_object_mut);
    for property in properties.into_iter().flat_map(|map| map.values_mut()) {
        let single = property
            .get("enum")
            .and_then(Value::as_array)
            .is_some_and(|values| values.len() == 1);
        if let (true, Some(map)) = (single, property.as_object_mut()) {
            map.insert(CLOSED.to_owned(), Value::Bool(true));
        }
    }
}

fn collect_union_refs(node: &Value, members: &mut BTreeSet<String>) {
    match node {
        Value::Object(map) => {
            let branches = map.get("oneOf").and_then(Value::as_array);
            let refs = branches
                .into_iter()
                .flatten()
                .filter_map(|branch| branch.get("$ref"));
            members.extend(refs.filter_map(Value::as_str).map(str::to_owned));
            map.values()
                .for_each(|child| collect_union_refs(child, members));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|child| collect_union_refs(child, members)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn open_string_enum(schema: &mut Map<String, Value>) {
    if schema.remove(CLOSED).is_some() {
        return;
    }
    let is_string = schema.get("type") == Some(&json!("string"));
    let has_values = schema
        .get("enum")
        .and_then(Value::as_array)
        .is_some_and(|values| !values.is_empty());
    if !is_string || !has_values {
        return;
    }
    let values = schema.remove("enum").unwrap_or(Value::Null);
    schema.remove("type");
    schema.remove("format");
    schema.insert(
        "oneOf".to_owned(),
        json!([{ "type": "string", "enum": values }, { "type": "string" }]),
    );
}

fn is_object_branch(branch: &Value) -> bool {
    branch.get("$ref").is_some()
        || branch.get("type") == Some(&json!("object"))
        || branch.get("properties").is_some()
}

fn open_object_union(schema: &mut Map<String, Value>) {
    let all_objects = schema
        .get("oneOf")
        .and_then(Value::as_array)
        .is_some_and(|branches| !branches.is_empty() && branches.iter().all(is_object_branch));
    if !all_objects {
        return;
    }
    let mut union = Map::new();
    for key in ["oneOf", "discriminator"] {
        if let Some(value) = schema.remove(key) {
            union.insert(key.to_owned(), value);
        }
    }
    schema.insert("oneOf".to_owned(), json!([Value::Object(union), {}]));
}

fn is_deprecated(node: &Value) -> bool {
    node.get("deprecated") == Some(&Value::Bool(true))
}

fn drop_deprecated(spec: &mut Value) {
    for item in operations(spec) {
        item.retain(|_, operation| !is_deprecated(operation));
        for operation in item.values_mut() {
            if let Some(parameters) = operation
                .get_mut("parameters")
                .and_then(Value::as_array_mut)
            {
                parameters.retain(|parameter| !is_deprecated(parameter));
            }
        }
    }
    walk(spec, &mut drop_deprecated_properties);
}

fn drop_deprecated_properties(schema: &mut Map<String, Value>) {
    let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) else {
        return;
    };
    let gone: Vec<String> = properties
        .iter()
        .filter(|(_, property)| is_deprecated(property))
        .map(|(name, _)| name.clone())
        .collect();
    properties.retain(|name, _| !gone.contains(name));
    if let Some(required) = schema.get_mut("required").and_then(Value::as_array_mut) {
        required.retain(|name| {
            name.as_str()
                .is_none_or(|name| !gone.iter().any(|g| g == name))
        });
    }
}
