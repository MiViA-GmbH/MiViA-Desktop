use serde_json::Value;

const LINE_WIDTH: usize = 120;
const TAB_WIDTH: usize = 2;

fn scalar(value: &Value) -> bool {
    !matches!(value, Value::Array(_) | Value::Object(_))
}

fn inline(items: &[Value]) -> String {
    let parts: Vec<String> = items.iter().map(Value::to_string).collect();
    format!("[{}]", parts.join(", "))
}

fn write(value: &Value, depth: usize, column: usize, out: &mut String) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            out.push_str("{\n");
            let last = map.len() - 1;
            for (at, (key, inner)) in map.iter().enumerate() {
                out.push_str(&"\t".repeat(depth + 1));
                let head = format!("{}: ", Value::String(key.clone()));
                out.push_str(&head);
                let used = (depth + 1) * TAB_WIDTH + head.len();
                write(inner, depth + 1, used, out);
                out.push_str(if at == last { "\n" } else { ",\n" });
            }
            out.push_str(&"\t".repeat(depth));
            out.push('}');
        }
        Value::Array(items) if !items.is_empty() => {
            let flat = inline(items);
            if items.iter().all(scalar) && column + flat.len() < LINE_WIDTH {
                out.push_str(&flat);
                return;
            }
            out.push_str("[\n");
            let last = items.len() - 1;
            for (at, item) in items.iter().enumerate() {
                out.push_str(&"\t".repeat(depth + 1));
                write(item, depth + 1, (depth + 1) * TAB_WIDTH, out);
                out.push_str(if at == last { "\n" } else { ",\n" });
            }
            out.push_str(&"\t".repeat(depth));
            out.push(']');
        }
        Value::Object(_)
        | Value::Array(_)
        | Value::Null
        | Value::Bool(_)
        | Value::Number(_)
        | Value::String(_) => out.push_str(&value.to_string()),
    }
}

pub(crate) fn biome(value: &Value) -> String {
    let mut out = String::new();
    write(value, 0, 0, &mut out);
    out.push('\n');
    out
}
