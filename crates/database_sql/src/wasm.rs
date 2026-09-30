//! The engine, as the browser calls it.
//!
//! One entry point: [`Query`], a statement held open between steps. A driver
//! constructs one with the catalog and the statement, reads the first
//! [`Step`] from [`Query::start`], serves each request, and feeds the pages,
//! bins or op results back until a step is `done`. Values cross as plain JSON
//! objects in the shapes `serde` gives the engine's types, which
//! `bin/database_sql_types.rs` writes out as TypeScript for
//! `apps/web/src/lib/core/database-sql/wasm-module.ts`.
//!
//! Only the wasm-bindgen glue lives here; the engine knows nothing of it.

use models_databases::OpResult;
use serde::Serialize;
use serde_wasm_bindgen::Serializer;
use wasm_bindgen::prelude::*;

use crate::catalog::Catalog;
use crate::engine::{Engine, Step};
use crate::fold::Bin;
use crate::run::Page;

/// One statement in flight.
#[wasm_bindgen]
pub struct Query {
    engine: Engine,
    first: Option<Step>,
}

#[wasm_bindgen]
impl Query {
    /// Compile `sql` against `catalog` (a `Catalog` as JSON).
    ///
    /// # Errors
    ///
    /// Returns a JS string when the catalog cannot be read or the statement
    /// does not compile; the string is what the agent should read.
    #[wasm_bindgen(constructor)]
    pub fn new(catalog: JsValue, sql: &str) -> Result<Query, JsValue> {
        let catalog: Catalog = serde_wasm_bindgen::from_value(catalog)
            .map_err(|error| JsValue::from_str(&format!("catalog is not readable: {error}")))?;
        let (engine, first) =
            Engine::start(&catalog, sql).map_err(|error| JsValue::from_str(&error.to_string()))?;
        Ok(Self {
            engine,
            first: Some(first),
        })
    }

    /// The first step. Taken once; a second call is an error.
    pub fn start(&mut self) -> Result<JsValue, JsValue> {
        let step = self
            .first
            .take()
            .ok_or_else(|| JsValue::from_str("the query has already started"))?;
        to_js(&step)
    }

    /// Feed one page (`{rows, next}`) of the outstanding request.
    pub fn feed_page(&mut self, request_id: u32, page: JsValue) -> Result<JsValue, JsValue> {
        let page: Page = serde_wasm_bindgen::from_value(page)
            .map_err(|error| JsValue::from_str(&format!("page is not readable: {error}")))?;
        let step = self
            .engine
            .feed_page(request_id, page)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        to_js(&step)
    }

    /// Feed the results (`[{kind, …}]`, one per op) of the outstanding ops.
    pub fn feed_ops(&mut self, request_id: u32, results: JsValue) -> Result<JsValue, JsValue> {
        let results: Vec<OpResult> = serde_wasm_bindgen::from_value(results)
            .map_err(|error| JsValue::from_str(&format!("results are not readable: {error}")))?;
        let step = self
            .engine
            .feed_ops(request_id, results)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        to_js(&step)
    }

    /// Feed the bins (`[{key, count}]`) of the outstanding request.
    pub fn feed_bins(&mut self, request_id: u32, bins: JsValue) -> Result<JsValue, JsValue> {
        let bins: Vec<Bin> = serde_wasm_bindgen::from_value(bins)
            .map_err(|error| JsValue::from_str(&format!("bins are not readable: {error}")))?;
        let step = self
            .engine
            .feed_bins(request_id, bins)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        to_js(&step)
    }
}

/// Plain objects and arrays, as JSON would give them: maps become objects
/// rather than JS `Map`s, so a row's cells read as `cells[key]`.
fn to_js(value: &impl Serialize) -> Result<JsValue, JsValue> {
    value
        .serialize(&Serializer::json_compatible())
        .map_err(|error| JsValue::from_str(&error.to_string()))
}
