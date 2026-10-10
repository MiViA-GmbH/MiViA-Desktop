#[path = "src/spec_patch.rs"]
mod spec_patch;

use std::path::PathBuf;

const SNAPSHOT: &str = "openapi.snapshot.json";
const LINT_SCOPE: &str = "#[allow(unreachable_pub, unused_qualifications, unused_lifetimes,      redundant_lifetimes, let_underscore_drop, clippy::all, clippy::pedantic, clippy::nursery, clippy::restriction)]";

fn generate() -> Result<String, Box<dyn std::error::Error>> {
    let mut spec: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(SNAPSHOT)?)?;
    spec_patch::apply(&mut spec);
    let spec = serde_json::from_value(spec)?;
    let mut settings = progenitor::GenerationSettings::default();
    settings
        .with_interface(progenitor::InterfaceStyle::Positional)
        .with_inner_type("crate::Call".parse::<proc_macro2::TokenStream>()?);
    let tokens = progenitor::Generator::new(&settings).generate_tokens(&spec)?;
    let code = prettyplease::unparse(&syn::parse2(tokens)?);
    Ok(format!(
        "{LINT_SCOPE}
pub mod api {{
{code}
}}
"
    ))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo::rerun-if-changed={SNAPSHOT}");
    println!("cargo::rerun-if-changed=src/spec_patch.rs");
    let out = PathBuf::from(std::env::var("OUT_DIR")?).join("codegen.rs");
    std::fs::write(out, generate()?)?;
    Ok(())
}
