//! Engine-owned schema metadata supplied by the frontend bundle.
//! The compiled schema is a bootstrap baseline, not the lifetime schema of a
//! native binary. Runtime metadata can extend it without changing record keys.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

/// Kind of a composite (selectable) type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeKind {
    Object,
    Union,
    Interface,
}

/// What a field's named type resolves to, decided at codegen time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldKind {
    /// Object / union / interface — value is a link (ref) or embedded object.
    Composite,
    /// Built-in scalar or enum — stored as a plain leaf value.
    Leaf,
    /// Custom scalar (e.g. `JSON`) — stored as an opaque JSON blob.
    OpaqueScalar,
}

/// Flattened GraphQL wrapping type. Nested lists are rejected at codegen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldType {
    /// The named (inner) type.
    pub name: String,
    pub kind: FieldKind,
    /// Nullability of the outermost wrapper.
    pub nullable: bool,
    pub list: bool,
    /// Only meaningful when `list` is true.
    pub item_nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldMeta {
    pub name: String,
    pub ty: FieldType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeMeta {
    pub name: String,
    pub kind: TypeKind,
    /// `Some(fields)` when the type is a normalized entity; `None` when it is
    /// embedded inline in its parent record. Derived from the schema's
    /// `id: ID!` convention.
    pub key_fields: Option<Vec<String>>,
    /// Field definitions (objects/interfaces only; empty for unions).
    pub fields: Vec<FieldMeta>,
    /// Union members / interface implementors (empty for plain objects).
    pub possible_types: Vec<String>,
}

include!(concat!(env!("OUT_DIR"), "/schema_meta.rs"));

/// Version of the metadata format and the cache interpretation rules.
pub const SCHEMA_PROTOCOL_VERSION: u32 = 1;
/// Bound the untrusted bridge payload before deserialization.
pub const MAX_SCHEMA_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SchemaDescriptor {
    protocol_version: u32,
    query_root: String,
    mutation_root: Option<String>,
    subscription_root: Option<String>,
    types: Vec<TypeMeta>,
}

/// Validated immutable metadata. Each engine owns its own schema snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schema {
    descriptor: SchemaDescriptor,
    fingerprint: String,
}

#[derive(Debug, thiserror::Error)]
#[error("incompatible cache schema: {0}")]
pub struct SchemaError(pub String);

/// Explicit handshake acknowledgement; old native engines return no such value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaAcknowledgement {
    pub protocol_version: u32,
    pub fingerprint: String,
}

static COMPILED: LazyLock<Schema> = LazyLock::new(|| {
    Schema::validate(SchemaDescriptor {
        protocol_version: SCHEMA_PROTOCOL_VERSION,
        query_root: QUERY_ROOT_TYPE.into(),
        mutation_root: MUTATION_ROOT_TYPE.map(str::to_owned),
        subscription_root: SUBSCRIPTION_ROOT_TYPE.map(str::to_owned),
        types: compiled_types(),
    })
    .expect("compiled schema is valid")
});

impl Schema {
    pub fn compiled() -> &'static Self {
        &COMPILED
    }

    pub fn from_json(json: &str) -> Result<Self, SchemaError> {
        if json.len() > MAX_SCHEMA_BYTES {
            return Err(SchemaError("metadata exceeds size limit".into()));
        }
        let descriptor =
            serde_json::from_str(json).map_err(|error| SchemaError(error.to_string()))?;
        Self::validate(descriptor)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.descriptor).expect("schema serializes")
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    pub fn acknowledgement(&self) -> SchemaAcknowledgement {
        SchemaAcknowledgement {
            protocol_version: SCHEMA_PROTOCOL_VERSION,
            fingerprint: self.fingerprint.clone(),
        }
    }
    pub fn query_root(&self) -> &str {
        &self.descriptor.query_root
    }
    pub fn mutation_root(&self) -> Option<&str> {
        self.descriptor.mutation_root.as_deref()
    }
    pub fn subscription_root(&self) -> Option<&str> {
        self.descriptor.subscription_root.as_deref()
    }
    pub fn type_meta(&self, name: &str) -> Option<&TypeMeta> {
        self.descriptor
            .types
            .binary_search_by(|ty| ty.name.as_str().cmp(name))
            .ok()
            .map(|index| &self.descriptor.types[index])
    }
    pub fn field_meta(&self, type_name: &str, field: &str) -> Option<&FieldMeta> {
        self.type_meta(type_name)?
            .fields
            .iter()
            .find(|candidate| candidate.name == field)
    }
    pub fn type_matches(&self, concrete: &str, condition: &str) -> bool {
        concrete == condition
            || self
                .type_meta(condition)
                .is_some_and(|ty| ty.possible_types.iter().any(|name| name == concrete))
    }

    /// Union compatible additions. An older window cannot downgrade metadata
    /// already accepted by another window or persisted by a newer bundle.
    pub fn merge(&self, incoming: &Self) -> Result<Self, SchemaError> {
        let mut descriptor = self.descriptor.clone();
        if descriptor.query_root != incoming.descriptor.query_root
            || descriptor.mutation_root != incoming.descriptor.mutation_root
            || descriptor.subscription_root != incoming.descriptor.subscription_root
        {
            return Err(SchemaError("operation roots changed".into()));
        }
        for added in &incoming.descriptor.types {
            let Some(existing) = descriptor.types.iter_mut().find(|ty| ty.name == added.name)
            else {
                descriptor.types.push(added.clone());
                continue;
            };
            if existing.kind != added.kind || existing.key_fields != added.key_fields {
                return Err(SchemaError(format!(
                    "type/key policy changed for {}",
                    added.name
                )));
            }
            for field in &added.fields {
                match existing.fields.iter().find(|old| old.name == field.name) {
                    Some(old) if old != field => {
                        return Err(SchemaError(format!(
                            "field shape changed for {}.{}",
                            added.name, field.name
                        )));
                    }
                    Some(_) => {}
                    None => existing.fields.push(field.clone()),
                }
            }
            for possible in &added.possible_types {
                if !existing.possible_types.contains(possible) {
                    existing.possible_types.push(possible.clone());
                }
            }
        }
        Self::validate(descriptor)
    }

    fn validate(mut descriptor: SchemaDescriptor) -> Result<Self, SchemaError> {
        if descriptor.protocol_version != SCHEMA_PROTOCOL_VERSION {
            return Err(SchemaError(format!(
                "unsupported protocol {}; update the app",
                descriptor.protocol_version
            )));
        }
        fn names<'a>(values: impl IntoIterator<Item = &'a str>) -> Result<(), SchemaError> {
            let mut seen = std::collections::BTreeSet::new();
            for value in values {
                let mut chars = value.chars();
                if value.starts_with("__")
                    || value.len() > 256
                    || !chars
                        .next()
                        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
                    || !chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
                    || !seen.insert(value)
                {
                    return Err(SchemaError(format!("invalid or duplicate name {value:?}")));
                }
            }
            Ok(())
        }
        names(descriptor.types.iter().map(|ty| ty.name.as_str()))?;
        descriptor.types.sort_by(|a, b| a.name.cmp(&b.name));
        for ty in &mut descriptor.types {
            names(ty.fields.iter().map(|field| field.name.as_str()))?;
            names(ty.possible_types.iter().map(String::as_str))?;
            ty.fields.sort_by(|a, b| a.name.cmp(&b.name));
            ty.possible_types.sort();
            if (ty.kind == TypeKind::Union && !ty.fields.is_empty())
                || (ty.kind == TypeKind::Object && !ty.possible_types.is_empty())
            {
                return Err(SchemaError(format!("invalid type shape {}", ty.name)));
            }
            let id = ty.fields.iter().find(|field| field.name == "id");
            let expected_keys = id.map(|_| vec!["id".to_string()]);
            if ty.key_fields != expected_keys
                || id.is_some_and(|field| {
                    field.ty.name != "ID"
                        || field.ty.kind != FieldKind::Leaf
                        || field.ty.list
                        || field.ty.nullable
                })
            {
                return Err(SchemaError(format!("invalid key policy for {}", ty.name)));
            }
        }
        let mut schema = Self {
            descriptor,
            fingerprint: String::new(),
        };
        for root in [
            Some(schema.query_root()),
            schema.mutation_root(),
            schema.subscription_root(),
        ]
        .into_iter()
        .flatten()
        {
            if !schema
                .type_meta(root)
                .is_some_and(|ty| ty.kind == TypeKind::Object && ty.key_fields.is_none())
            {
                return Err(SchemaError(format!("invalid operation root {root}")));
            }
        }
        for ty in &schema.descriptor.types {
            for possible in &ty.possible_types {
                if !schema
                    .type_meta(possible)
                    .is_some_and(|ty| ty.kind == TypeKind::Object)
                {
                    return Err(SchemaError(format!("unknown concrete type {possible}")));
                }
            }
            for field in &ty.fields {
                names([field.ty.name.as_str()])?;
                if (field.ty.kind == FieldKind::Composite)
                    != schema.type_meta(&field.ty.name).is_some()
                    || (!field.ty.list && field.ty.item_nullable)
                {
                    return Err(SchemaError(format!(
                        "invalid field shape {}.{}",
                        ty.name, field.name
                    )));
                }
            }
        }
        let json = schema.to_json();
        if json.len() > MAX_SCHEMA_BYTES {
            return Err(SchemaError("merged metadata exceeds size limit".into()));
        }
        schema.fingerprint = format!("{:x}", Sha256::digest(json.as_bytes()));
        Ok(schema)
    }
}

/// Compiled-default helpers for standalone consumers. Engine operations use
/// their explicitly scoped schema instead.
pub fn type_meta(name: &str) -> Option<&'static TypeMeta> {
    Schema::compiled().type_meta(name)
}
pub fn field_meta(type_name: &str, field: &str) -> Option<&'static FieldMeta> {
    Schema::compiled().field_meta(type_name, field)
}
pub fn type_matches(concrete: &str, condition: &str) -> bool {
    Schema::compiled().type_matches(concrete, condition)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_root_is_present() {
        let root = type_meta(QUERY_ROOT_TYPE).expect("query root type");
        assert_eq!(root.kind, TypeKind::Object);
        assert!(root.key_fields.is_none());
        assert!(root.fields.iter().any(|f| f.name == "user"));

        // The viewer object is keyed by presence-of-id.
        let user = type_meta("GraphqlUser").expect("user type");
        assert_eq!(user.key_fields, Some(vec!["id".to_string()]));
        assert!(user.fields.iter().any(|f| f.name == "soup"));
    }

    #[test]
    fn mutation_root_is_present() {
        let name = MUTATION_ROOT_TYPE.expect("schema has a mutation root");
        let root = type_meta(name).expect("mutation root type");
        assert_eq!(root.kind, TypeKind::Object);
        assert!(root.key_fields.is_none());
        assert!(root.fields.iter().any(|f| f.name == "setEntityProperty"));
    }

    #[test]
    fn subscription_root_is_present() {
        let name = SUBSCRIPTION_ROOT_TYPE.expect("schema has a subscription root");
        let root = type_meta(name).expect("subscription root type");
        assert_eq!(root.kind, TypeKind::Object);
        assert!(root.key_fields.is_none());
        assert!(root.fields.iter().any(|field| field.name == "soupUpdates"));
    }

    #[test]
    fn interface_possible_types() {
        let entity = type_meta("GraphqlSoupEntity").expect("entity interface");
        assert_eq!(entity.kind, TypeKind::Interface);
        assert!(
            entity
                .possible_types
                .contains(&"GraphqlSoupDocument".to_string())
        );
        assert!(
            entity
                .possible_types
                .contains(&"GraphqlSoupForeignEntity".to_string())
        );
        assert!(
            entity
                .possible_types
                .contains(&"GraphqlSoupCalendarEvent".to_string())
        );
        assert!(
            entity
                .possible_types
                .contains(&"GraphqlSoupReminder".to_string())
        );
        assert!(
            entity
                .possible_types
                .contains(&"GraphqlSoupAgentSession".to_string())
        );
        assert!(
            entity
                .possible_types
                .contains(&"GraphqlSoupInitiative".to_string())
        );
        assert_eq!(entity.possible_types.len(), 13);
        assert!(type_matches("GraphqlSoupDocument", "GraphqlSoupEntity"));
        assert!(type_matches("GraphqlSoupInitiative", "GraphqlSoupEntity"));
        assert!(!type_matches("GraphqlSoupItem", "GraphqlSoupEntity"));
    }

    #[test]
    fn presence_of_id_convention_applied() {
        assert_eq!(
            type_meta("GraphqlSoupDocument").unwrap().key_fields,
            Some(vec!["id".to_string()])
        );
        // Renamed from `messageId` so the convention keys it.
        assert_eq!(
            type_meta("GraphqlSoupChannelMessage").unwrap().key_fields,
            Some(vec!["id".to_string()])
        );
        // Property assignments have globally unique database ids and are
        // normalized independently from their shared definitions.
        assert_eq!(
            type_meta("GraphqlProperty").unwrap().key_fields,
            Some(vec!["id".to_string()])
        );
        assert!(field_meta("GraphqlProperty", "propertyDefinitionId").is_some());
        // No id field → embedded.
        assert_eq!(type_meta("SoupPage").unwrap().key_fields, None);
        assert_eq!(
            type_meta("GraphqlSoupChannelParticipant")
                .unwrap()
                .key_fields,
            None
        );
    }

    #[test]
    fn field_shapes() {
        // properties: [GraphqlProperty!]!
        let f = field_meta("GraphqlSoupDocument", "properties").unwrap();
        assert_eq!(f.ty.name, "GraphqlProperty");
        assert_eq!(f.ty.kind, FieldKind::Composite);
        assert!(!f.ty.nullable && f.ty.list && !f.ty.item_nullable);

        // viewedAt: String (nullable leaf)
        let f = field_meta("GraphqlSoupDocument", "viewedAt").unwrap();
        assert_eq!(f.ty.kind, FieldKind::Leaf);
        assert!(f.ty.nullable && !f.ty.list);

        // sourceMetadata: JSON! (opaque scalar); metadata is now the shared
        // structured interface field.
        let f = field_meta("GraphqlSoupForeignEntity", "sourceMetadata").unwrap();
        assert_eq!(f.ty.kind, FieldKind::OpaqueScalar);
        assert!(!f.ty.nullable);

        // items: [GraphqlSoupEntity!]! (composite link to the entity interface)
        let f = field_meta("SoupPage", "items").unwrap();
        assert_eq!(f.ty.kind, FieldKind::Composite);
        assert_eq!(f.ty.name, "GraphqlSoupEntity");
        assert!(f.ty.list);
    }

    #[test]
    fn schema_hash_present() {
        assert_eq!(SCHEMA_HASH.len(), 64);
    }
}
