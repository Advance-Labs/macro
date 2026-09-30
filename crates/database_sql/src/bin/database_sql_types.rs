//! Export the engine's wire contract as TypeScript.
//!
//! Run with:
//!
//! ```text
//! cargo run -p database_sql --features cli --bin database_sql_types
//! ```

use database_sql::catalog::Schema;
use database_sql::{Bin, Catalog, Page, Step};
use models_databases::OpResult;
use specta::Types;
use specta::datatype::{DataType, Fields};
use specta_typescript::Typescript;
use specta_typescript::semantic::Configuration;
use std::fs;
use std::path::Path;

// `specta_serde`'s spellings of the two field attributes; the crate is pinned.
const FIELD_DEFAULT: &str = "serde:field:default";
const FIELD_SKIP_SERIALIZING_IF: &str = "serde:field:skip_serializing_if";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // What `wasm::Query` and `wasm::build_catalog` read and return;
    // everything else is reached from these.
    let types = Types::default()
        .register::<Schema>()
        .register::<Catalog>()
        .register::<Step>()
        .register::<Page>()
        .register::<Bin>()
        .register::<OpResult>();
    // serde-wasm-bindgen hands `NaN` and the infinities across as numbers,
    // so an `f64` is a plain `number` rather than JSON's `number | null`.
    let types = Configuration::empty()
        .enable_lossless_floats()
        .apply_types(&types)
        .into_owned();
    let types = symmetric_omissions(types);
    let types = apart_from_the_catalog(types);
    let output = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/src/lib/core/database-sql/generated/types.ts");

    let generated = Typescript::default().export(&types, specta_serde::Format)?;
    let generated = generated
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    fs::create_dir_all(output.parent().ok_or("the output has a directory")?)?;
    fs::write(output, format!("{generated}\n"))?;
    Ok(())
}

/// The ops name a column type and an entity kind as the catalog does, but
/// mean something narrower (no options, no relations); TypeScript gets them
/// under names of their own.
fn apart_from_the_catalog(types: Types) -> Types {
    types.map(|mut named| {
        if named.module_path.starts_with("models_databases") {
            match named.name.as_ref() {
                "ColumnKind" => named.name = "OpColumnKind".into(),
                "EntityKind" => named.name = "OpEntityKind".into(),
                _ => {}
            }
        }
        named
    })
}

/// `specta_serde::Format` refuses every `skip_serializing_if`, because an
/// omission on write alone would make the two directions differ. A field
/// that is also `default` may be absent both ways, so it is one optional
/// field and the attribute says nothing more; drop it there. A field without
/// `default` keeps it, and the export fails on it.
fn symmetric_omissions(types: Types) -> Types {
    types.map(|mut named| {
        if let Some(DataType::Struct(structure)) = &mut named.ty
            && let Fields::Named(fields) = &mut structure.fields
        {
            for (_, field) in &mut fields.fields {
                if field.attributes.contains_key(FIELD_DEFAULT) {
                    field.attributes.remove(FIELD_SKIP_SERIALIZING_IF);
                }
            }
        }
        named
    })
}
