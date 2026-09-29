//! The engine over the real GraphQL API of a running Macro stack, with your
//! tasks as the table `macro.tasks`.
//!
//! ```sh
//! cargo run -p database_sql --example gql -- --url http://localhost:32015 --token-file token
//! ```
//!
//! The token is a user JWT; locally the passwordless flow hands one out:
//!
//! ```sh
//! code=$(curl -s $AUTH/login/passwordless -H content-type:application/json \
//!   -d '{"email":"you@example.com","redirect_uri":"http://localhost/app"}' | jq -r .code)
//! curl -s "$AUTH/oauth/passwordless/$code?email=you%40example.com&disable_redirect=true" | jq -r .access_token > token
//! ```
//!
//! Reads go through `Query.soup` / `Query.groupSoup` with the plan's `propf`
//! pushed down exactly as the browser will send it; writes go through
//! `setEntityProperty` and `POST /documents/create_task`. Every task property
//! definition the API returns is a column; the task title is the `name`
//! column.

mod common;

use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use chrono::DateTime;
use database_sql::catalog::{Catalog, Column, ColumnKind, SelectOption, Table, TableSource};
use database_sql::fold::{Bin, Cell, Row};
use database_sql::resolve::Value;
use database_sql::run::{Page, RowSource, RowWriter, SourceError, WriteError, run};
use database_sql::split::GqlQuery;
use filter_ast::Expr;
use item_filters::ast::properties::{PropertiesLiteral, PropertyMatchValue};
use serde_json::{Value as Json, json};
use uuid::Uuid;

const TASKS: Uuid = Uuid::from_u128(0x7a5c);
/// The task title is not a property; it gets a column of its own.
const NAME: Uuid = Uuid::from_u128(0x7a5c_0001);
const NIL: &str = "00000000-0000-0000-0000-000000000000";

const PROPERTY_FIELDS: &str = r#"
fragment SoupPropertyFields on GraphqlProperty {
  propertyDefinitionId
  value {
    __typename
    ... on GraphqlBooleanPropertyValue { boolValue: value }
    ... on GraphqlNumberPropertyValue { numberValue: value }
    ... on GraphqlStringPropertyValue { stringValue: value }
    ... on GraphqlDatePropertyValue { dateValue: value }
    ... on GraphqlSelectOptionPropertyValue { optionIds }
    ... on GraphqlEntityReferencePropertyValue { references { entityId entityType } }
    ... on GraphqlLinkPropertyValue { urls }
  }
}"#;

struct Api {
    base: String,
    token: String,
    http: reqwest::Client,
    /// Per definition: multi-valued, and the entity type references take.
    definitions: HashMap<Uuid, (bool, String)>,
}

impl Api {
    async fn gql(&self, query: &str, variables: Json) -> Result<Json, String> {
        let response = self
            .http
            .post(format!("{}/items/soup/graphql", self.base))
            .bearer_auth(&self.token)
            .json(&json!({ "query": query, "variables": variables }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status();
        let body: Json = response.json().await.map_err(|e| e.to_string())?;
        if let Some(errors) = body.get("errors") {
            return Err(errors.to_string());
        }
        if !status.is_success() {
            return Err(format!("{status}: {body}"));
        }
        Ok(body["data"].clone())
    }

    /// Every task property definition as a column, plus the title.
    async fn catalog(&mut self) -> Result<Catalog, String> {
        let data = self
            .gql(
                r#"query { user { propertyDefinitions(scope: ALL, forEntityType: TASK) {
                     id displayName dataType isMultiSelect specificEntityType isMetadata
                     options { id displayOrder value { __typename
                       ... on GraphqlStringPropertyOptionValue { s: value }
                       ... on GraphqlNumberPropertyOptionValue { n: value } } }
                   } } }"#,
                json!({}),
            )
            .await?;
        let mut columns = vec![Column {
            id: NAME,
            name: "name".into(),
            kind: ColumnKind::Text,
        }];
        for definition in data["user"]["propertyDefinitions"]
            .as_array()
            .unwrap_or(&vec![])
        {
            if definition["isMetadata"].as_bool().unwrap_or(false) {
                continue;
            }
            let id: Uuid = definition["id"].as_str().unwrap().parse().unwrap();
            let multi = definition["isMultiSelect"].as_bool().unwrap_or(false);
            let entity_type = definition["specificEntityType"]
                .as_str()
                .unwrap_or("USER")
                .to_owned();
            self.definitions.insert(id, (multi, entity_type));
            let mut options: Vec<(i64, SelectOption)> = definition["options"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|option| {
                    let label = option["value"]["s"]
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| option["value"]["n"].as_f64().map(|n| n.to_string()))
                        .unwrap_or_default();
                    (
                        option["displayOrder"].as_i64().unwrap_or(0),
                        SelectOption {
                            id: option["id"].as_str().unwrap().parse().unwrap(),
                            label,
                        },
                    )
                })
                .collect();
            options.sort_by_key(|(order, _)| *order);
            let kind = match definition["dataType"].as_str().unwrap_or("") {
                "BOOLEAN" => ColumnKind::Boolean,
                "DATE" => ColumnKind::Date,
                "NUMBER" => ColumnKind::Number,
                "STRING" => ColumnKind::Text,
                "LINK" => ColumnKind::Link,
                "ENTITY" => ColumnKind::Entity { multi },
                _ => ColumnKind::Select {
                    multi,
                    options: options.into_iter().map(|(_, option)| option).collect(),
                },
            };
            columns.push(Column {
                id,
                name: definition["displayName"].as_str().unwrap().to_owned(),
                kind,
            });
        }
        Ok(Catalog {
            tables: vec![Table {
                id: TASKS,
                database: "macro".into(),
                name: "tasks".into(),
                columns,
                source: TableSource::Database,
            }],
        })
    }
}

/// The Soup `propf` input, from the plan's expression.
fn propf_input(expr: &Expr<PropertiesLiteral>) -> Json {
    match expr {
        Expr::And(a, b) => json!({ "and": { "left": propf_input(a), "right": propf_input(b) } }),
        Expr::Or(a, b) => json!({ "or": { "left": propf_input(a), "right": propf_input(b) } }),
        Expr::Not(a) => json!({ "not": propf_input(a) }),
        Expr::Literal(literal) => {
            let value = match &literal.value {
                PropertyMatchValue::SelectOption(id) => json!({ "selectOption": id }),
                PropertyMatchValue::EntityRef(id) => json!({ "entityRef": id.to_string() }),
            };
            json!({ "literal": { "propertyDefinitionId": literal.property_definition_id, "value": value } })
        }
    }
}

/// Tasks only: every other Soup entity type is excluded with an impossible
/// id, the way the Tasks view does it.
fn filters(propf: &Option<Expr<PropertiesLiteral>>) -> Json {
    let mut filters = json!({
        "documentFilter": { "literal": { "subType": "TASK" } },
        "projectFilter": { "literal": { "projectId": NIL } },
        "chatFilter": { "literal": { "chatId": NIL } },
        "emailFilter": { "tree": { "literal": { "threadId": NIL } } },
        "channelFilter": { "literal": { "channelId": NIL } },
        "channelThreadFilter": { "literal": { "threadId": NIL } },
        "callFilter": { "literal": { "callId": NIL } },
        "crmCompanyFilter": { "literal": { "id": NIL } },
        "foreignEntityFilter": { "literal": { "id": NIL } },
        "calendarEventFilter": { "literal": { "id": NIL } },
    });
    if let Some(expr) = propf {
        filters["propertiesFilter"] = propf_input(expr);
    }
    filters
}

/// A Soup item's properties as cells.
fn row_from_item(item: &Json) -> Row {
    let id = item["id"]
        .as_str()
        .and_then(|id| id.parse().ok())
        .unwrap_or_else(|| {
            Uuid::new_v5(
                &Uuid::NAMESPACE_OID,
                item["id"].as_str().unwrap_or("").as_bytes(),
            )
        });
    let mut cells = HashMap::new();
    if let Some(name) = item["documentName"].as_str() {
        cells.insert(NAME, Cell::Text(name.to_owned()));
    }
    for property in item["properties"].as_array().unwrap_or(&vec![]) {
        let Some(definition) = property["propertyDefinitionId"]
            .as_str()
            .and_then(|id| id.parse::<Uuid>().ok())
        else {
            continue;
        };
        let value = &property["value"];
        let cell = match value["__typename"].as_str().unwrap_or("") {
            "GraphqlBooleanPropertyValue" => value["boolValue"].as_bool().map(Cell::Bool),
            "GraphqlNumberPropertyValue" => value["numberValue"].as_f64().map(Cell::Number),
            "GraphqlStringPropertyValue" => value["stringValue"]
                .as_str()
                .map(|s| Cell::Text(s.to_owned())),
            "GraphqlDatePropertyValue" => value["dateValue"]
                .as_str()
                .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
                .map(|d| Cell::Date(d.to_utc())),
            "GraphqlSelectOptionPropertyValue" => Some(Cell::Options(
                value["optionIds"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|id| id.as_str().and_then(|id| id.parse().ok()))
                    .collect(),
            )),
            "GraphqlEntityReferencePropertyValue" => Some(Cell::Entities(
                value["references"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|reference| reference["entityId"].as_str().map(str::to_owned))
                    .collect(),
            )),
            "GraphqlLinkPropertyValue" => value["urls"]
                .as_array()
                .and_then(|urls| urls.first())
                .and_then(|url| url.as_str())
                .map(|url| Cell::Text(url.to_owned())),
            _ => None,
        };
        if let Some(cell) = cell {
            cells.insert(definition, cell);
        }
    }
    Row { id, cells }
}

impl RowSource for Api {
    async fn page(
        &self,
        query: &GqlQuery,
        _needs: &[Uuid],
        cursor: Option<String>,
        limit: usize,
    ) -> Result<Page, SourceError> {
        let GqlQuery::Soup { propf, .. } = query else {
            return Err(SourceError("page needs a soup query".into()));
        };
        let input = match cursor {
            Some(cursor) => {
                json!({ "continuation": { "cursor": cursor, "expand": true, "sortDirection": "DESC" } })
            }
            None => json!({ "initial": {
                "limit": limit.min(500), "expand": true, "sortMethod": "UPDATED_AT", "sortDirection": "DESC",
                "filters": filters(propf),
            } }),
        };
        let data = self
            .gql(
                &format!(
                    r#"query TaskSoup($input: SoupInput!) {{ user {{ soup(input: $input) {{
                         items {{ __typename id ... on GraphqlSoupDocument {{ documentName: name properties {{ ...SoupPropertyFields }} }} }}
                         nextCursor }} }} }} {PROPERTY_FIELDS}"#
                ),
                json!({ "input": input }),
            )
            .await
            .map_err(SourceError)?;
        let soup = &data["user"]["soup"];
        Ok(Page {
            rows: soup["items"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(row_from_item)
                .collect(),
            next: soup["nextCursor"].as_str().map(str::to_owned),
        })
    }

    async fn bins(&self, query: &GqlQuery) -> Result<Vec<Bin>, SourceError> {
        let GqlQuery::GroupSoup {
            propf, group_by, ..
        } = query
        else {
            return Err(SourceError("bins need a groupSoup query".into()));
        };
        let data = self
            .gql(
                r#"query GroupSoup($input: GroupedSoupInput!) { user { groupSoup(input: $input) {
                     bins { key totalCount } } } }"#,
                json!({ "input": { "initial": {
                    "groupBy": { "field": "PROPERTY", "propertyDefinitionId": group_by },
                    "limit": 1, "sortMethod": "UPDATED_AT", "filters": filters(propf),
                } } }),
            )
            .await
            .map_err(SourceError)?;
        Ok(data["user"]["groupSoup"]["bins"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|bin| {
                let key = bin["key"].as_str().unwrap_or("");
                let key = if key.is_empty() {
                    None
                } else if let Ok(id) = key.parse::<Uuid>() {
                    // Select bins are keyed by option id; entity bins by the
                    // entity id, which is never a bare UUID for users.
                    Some(Cell::Options(vec![id]))
                } else {
                    Some(Cell::Entities(vec![key.to_owned()]))
                };
                Bin {
                    key,
                    count: bin["totalCount"].as_u64().unwrap_or(0),
                }
            })
            .collect())
    }
}

impl Api {
    /// The `setEntityProperty` value input for one cell.
    fn property_input(&self, column: Uuid, value: Option<Value>) -> Json {
        let (multi, entity_type) = self
            .definitions
            .get(&column)
            .cloned()
            .unwrap_or((false, "USER".into()));
        match value {
            None => Json::Null,
            Some(Value::Text(text)) => json!({ "string": text }),
            Some(Value::Number(n)) => json!({ "number": n }),
            Some(Value::Bool(b)) => json!({ "boolean": b }),
            Some(Value::Date(d)) => json!({ "date": d.to_rfc3339() }),
            Some(Value::Option(id)) if multi => json!({ "multiSelectOption": [id] }),
            Some(Value::Option(id)) => json!({ "selectOption": id }),
            Some(Value::Entity(id)) if multi => {
                json!({ "multiEntityReference": [{ "entityType": entity_type, "entityId": id }] })
            }
            Some(Value::Entity(id)) => {
                json!({ "entityReference": { "entityType": entity_type, "entityId": id } })
            }
        }
    }

    async fn set(&self, task: Uuid, column: Uuid, value: Option<Value>) -> Result<(), WriteError> {
        if column == NAME {
            return Err(WriteError(
                "the task title is renamed in the app, not by SQL".into(),
            ));
        }
        self.gql(
            r#"mutation Set($input: SetEntityPropertyInput!) { setEntityProperty(input: $input) { id } }"#,
            json!({ "input": {
                "entityType": "DOCUMENT", "entityId": task, "propertyDefinitionId": column,
                "value": self.property_input(column, value),
            } }),
        )
        .await
        .map(|_| ())
        .map_err(WriteError)
    }
}

impl RowWriter for Api {
    async fn insert(&self, _table: Uuid, cells: Vec<(Uuid, Value)>) -> Result<Uuid, WriteError> {
        let name = cells
            .iter()
            .find_map(|(column, value)| match (column, value) {
                (column, Value::Text(text)) if *column == NAME => Some(text.clone()),
                _ => None,
            })
            .ok_or_else(|| WriteError("INSERT into macro.tasks needs a name".into()))?;
        let response = self
            .http
            .post(format!("{}/documents/create_task", self.base))
            .bearer_auth(&self.token)
            .json(&json!({ "taskName": name, "markdown": null, "shareWithTeam": true }))
            .send()
            .await
            .map_err(|e| WriteError(e.to_string()))?;
        let status = response.status();
        let body: Json = response
            .json()
            .await
            .map_err(|e| WriteError(e.to_string()))?;
        if !status.is_success() {
            return Err(WriteError(format!("create_task {status}: {body}")));
        }
        let task: Uuid = body["documentId"]
            .as_str()
            .and_then(|id| id.parse().ok())
            .ok_or_else(|| WriteError(format!("create_task answered without an id: {body}")))?;
        for (column, value) in cells {
            if column != NAME {
                self.set(task, column, Some(value)).await?;
            }
        }
        Ok(task)
    }

    async fn update(
        &self,
        _table: Uuid,
        row_id: Uuid,
        cells: Vec<(Uuid, Option<Value>)>,
    ) -> Result<(), WriteError> {
        for (column, value) in cells {
            self.set(row_id, column, value).await?;
        }
        Ok(())
    }

    async fn delete(&self, _table: Uuid, _row_id: Uuid) -> Result<(), WriteError> {
        Err(WriteError(
            "deleting tasks is not wired here; trash it in the app".into(),
        ))
    }
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let mut base = "http://localhost:32015".to_owned();
    let mut token_file = "databases-demo-token".to_owned();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--url" => base = args.next().expect("--url <base>"),
            "--token-file" => token_file = args.next().expect("--token-file <path>"),
            other => panic!("unknown argument {other}"),
        }
    }
    let token = std::fs::read_to_string(&token_file)
        .unwrap_or_else(|e| panic!("token file {token_file}: {e}"))
        .trim()
        .to_owned();
    let mut api = Api {
        base,
        token,
        http: reqwest::Client::new(),
        definitions: HashMap::new(),
    };
    let catalog = match api.catalog().await {
        Ok(catalog) => catalog,
        Err(error) => {
            eprintln!("could not load the catalog: {error}");
            std::process::exit(1);
        }
    };
    println!("database_sql over {} — table macro.tasks", api.base);
    println!(
        "columns: {}",
        catalog.tables[0]
            .columns
            .iter()
            .map(|column| format!("\"{}\"", column.name))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "try: SELECT name, Status, Priority, \"Due Date\" FROM macro.tasks WHERE Status = 'In Progress' ORDER BY \"Due Date\""
    );
    println!("     \\catalog  \\q");

    let stdin = io::stdin();
    loop {
        print!("sql> ");
        io::stdout().flush().unwrap();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap() == 0 {
            break;
        }
        let sql = line.trim();
        match sql {
            "" => continue,
            "\\q" => break,
            "\\catalog" => {
                for column in &catalog.tables[0].columns {
                    println!("  {:<24} {:?}", column.name, column.kind);
                }
                continue;
            }
            _ => {}
        }
        common::print_plan(&catalog, sql);
        match run(&catalog, sql, &api, &api).await {
            Ok(outcome) => common::print_outcome(&catalog, &outcome),
            Err(error) => println!("  error: {error}"),
        }
    }
}
