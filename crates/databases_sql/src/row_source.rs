//! The engine's row source on the server: a table's rows are read as Soup
//! database rows, with the same filters the browser's GraphQL source sends,
//! so agents and the grid see rows with one set of semantics. `people` are
//! the viewer's contacts.
//!
//! Rows come newest first: Soup's cursor orders by creation, so a statement
//! without `ORDER BY` lists rows in that order, as it does in the browser.

#[cfg(test)]
mod test;

use std::collections::HashMap;
use std::sync::Arc;

use contacts::domain::ports::ContactsService;
use database_sql::catalog::{Catalog, ColumnKind, PEOPLE_EMAIL, PEOPLE_ID, PEOPLE_NAME};
use database_sql::fold::{Bin, Cell, Row};
use database_sql::run::{Page, RowSource, SourceError};
use database_sql::split::{GqlQuery, KeyHint};
use email::domain::models::PreviewView;
use filter_ast::Expr;
use item_filters::ast::calendar_event::CalendarEventLiteral;
use item_filters::ast::call::CallLiteral;
use item_filters::ast::channel::{ChannelLiteral, ChannelThreadLiteral};
use item_filters::ast::chat::ChatLiteral;
use item_filters::ast::crm_company::CrmCompanyLiteral;
use item_filters::ast::database_row::DatabaseRowLiteral;
use item_filters::ast::document::DocumentLiteral;
use item_filters::ast::email::EmailLiteral;
use item_filters::ast::foreign_entity::ForeignEntityLiteral;
use item_filters::ast::project::ProjectLiteral;
use item_filters::ast::properties::{
    EntityRefId, PropertiesLiteral, PropertyEntityType, PropertyMatchValue,
};
use item_filters::ast::{EmailFilterAst, EntityFilterAst};
use macro_user_id::user_id::MacroUserIdStr;
use models_grouping::{GroupByField, GroupingConfig};
use models_pagination::{
    Base64Str, CursorWithValAndFilter, Query, SimpleSortMethod, TypeEraseCursor,
};
use models_properties::service::property_value::PropertyValue;
use models_soup::item::SoupItem;
use soup::domain::models::{
    GroupedSortRequest, SoupPropertiesField, SoupQuery, SoupRequest, SoupSortDirection, SoupType,
};
use soup::domain::ports::SoupService;
use uuid::Uuid;

/// Past this many join values a narrowed filter costs more than it saves.
const MAX_KEY_HINT_VALUES: usize = 100;

/// The rows of a statement's tables as `viewer` may read them.
pub(crate) struct SoupRowSource<'a, Soup, Contacts> {
    pub(crate) soup: &'a Soup,
    pub(crate) contacts: &'a Contacts,
    pub(crate) viewer: &'a MacroUserIdStr<'static>,
    /// The statement's catalog, for the value kinds of grouped columns.
    pub(crate) catalog: &'a Catalog,
}

impl<Soup, Contacts> RowSource for SoupRowSource<'_, Soup, Contacts>
where
    Soup: SoupService,
    Contacts: ContactsService,
{
    async fn page(
        &self,
        query: &GqlQuery,
        _needs: &[Uuid],
        cursor: Option<String>,
        limit: usize,
    ) -> Result<Page, SourceError> {
        match query {
            GqlQuery::Soup {
                table,
                propf,
                key_hint,
            } => {
                self.soup_page(*table, propf.as_ref(), key_hint.as_ref(), cursor, limit)
                    .await
            }
            GqlQuery::People { ids } => self.people_page(ids.as_deref()).await,
            GqlQuery::GroupSoup { .. } => Err(SourceError(
                "a grouped query is read as bins, not pages".into(),
            )),
        }
    }

    async fn bins(&self, query: &GqlQuery) -> Result<Vec<Bin>, SourceError> {
        let GqlQuery::GroupSoup {
            table,
            propf,
            group_by,
        } = query
        else {
            return Err(SourceError("only a grouped query has bins".into()));
        };
        let kind = self
            .catalog
            .tables
            .iter()
            .find(|candidate| candidate.id == *table)
            .and_then(|table| table.columns.iter().find(|column| column.id == *group_by))
            .map(|column| &column.kind)
            .ok_or_else(|| SourceError(format!("no column {group_by} in table {table}")))?;
        let request = GroupedSortRequest {
            // The bins' totals answer the count; one item each is enough.
            limit: 1,
            cursor: Query::Sort(
                SimpleSortMethod::CreatedAt,
                rows_filter(*table, propf.as_ref(), None),
            ),
            user_id: self.viewer.clone(),
            grouping: GroupingConfig {
                field: GroupByField::Property {
                    property_definition_id: *group_by,
                    entity_type: Some(PropertyEntityType::DatabaseRow.to_string()),
                },
                group_key: None,
                per_group_limit: Some(1),
            },
        };
        let items = self
            .soup
            .get_user_soup_grouped(request)
            .await
            .map_err(|error| SourceError(error.to_string()))?;
        let mut bins: Vec<Bin> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for item in items {
            if seen.contains(&item.key) {
                continue;
            }
            bins.push(Bin {
                key: bin_key(kind, &item.key, *group_by)?,
                count: item.total_group_count as u64,
            });
            seen.push(item.key);
        }
        Ok(bins)
    }
}

impl<Soup, Contacts> SoupRowSource<'_, Soup, Contacts>
where
    Soup: SoupService,
    Contacts: ContactsService,
{
    async fn soup_page(
        &self,
        table: Uuid,
        propf: Option<&Expr<PropertiesLiteral>>,
        key_hint: Option<&KeyHint>,
        cursor: Option<String>,
        limit: usize,
    ) -> Result<Page, SourceError> {
        let cursor = match cursor {
            None => SoupQuery::new_sort_simple(
                SimpleSortMethod::CreatedAt,
                rows_filter(table, propf, key_hint),
            ),
            Some(cursor) => SoupQuery::new_cursor_simple(
                Base64Str::<CursorWithValAndFilter<Uuid, SimpleSortMethod, EntityFilterAst>>::new_from_string(cursor)
                    .decode_json()
                    .map_err(|error| SourceError(format!("bad cursor: {error}")))?,
            ),
        };
        let request = SoupRequest {
            soup_type: SoupType::Expanded,
            limit: u16::try_from(limit).unwrap_or(u16::MAX),
            cursor,
            sort_direction: SoupSortDirection::Desc,
            user: self.viewer.clone(),
            // Every other kind is excluded, so emails never come into play.
            email_preview_view: PreviewView::default(),
            link_ids: Vec::new(),
        };
        let page = self
            .soup
            .get_user_soup_with_properties(request, None)
            .await
            .map_err(|error| SourceError(error.to_string()))?
            .type_erase();
        Ok(Page {
            rows: page
                .items
                .into_iter()
                .map(|enriched| table_row(enriched.item))
                .collect::<Result<_, _>>()?,
            next: page.next_cursor,
        })
    }

    async fn people_page(&self, ids: Option<&[String]>) -> Result<Page, SourceError> {
        let people = self
            .contacts
            .query_contacts(self.viewer.clone())
            .await
            .map_err(|error| SourceError(error.to_string()))?;
        Ok(Page {
            rows: people
                .into_iter()
                .filter(|person| ids.is_none_or(|ids| ids.iter().any(|id| id == person.as_ref())))
                .map(|person| person_row(&person))
                .collect(),
            next: None,
        })
    }
}

/// One person as a `people` row. People have no row ids of their own, so
/// the row is named by its user id in the OID namespace, as the browser
/// names it.
fn person_row(person: &MacroUserIdStr<'_>) -> Row {
    let id: &str = person.as_ref();
    let email = person.email_str().to_owned();
    let name = email.split('@').next().unwrap_or_default().to_owned();
    Row {
        id: Uuid::new_v5(&Uuid::NAMESPACE_OID, id.as_bytes()),
        position: None,
        cells: HashMap::from([
            (PEOPLE_ID, Cell::Entities(vec![id.to_owned()])),
            (PEOPLE_NAME, Cell::Text(name)),
            (PEOPLE_EMAIL, Cell::Text(email)),
        ]),
    }
}

/// A grouped key as the engine reads it. Soup files rows with an empty cell
/// under the empty key.
fn bin_key(kind: &ColumnKind, key: &str, column: Uuid) -> Result<Option<Cell>, SourceError> {
    if key.is_empty() {
        return Ok(None);
    }
    match kind {
        ColumnKind::Select { .. } => Uuid::parse_str(key)
            .map(|option| Some(Cell::Options(vec![option])))
            .map_err(|_| SourceError(format!("bin key {key} is not an option id"))),
        ColumnKind::Entity { .. } => Ok(Some(Cell::Entities(vec![key.to_owned()]))),
        _ => Err(SourceError(format!(
            "column {column} cannot be grouped by Soup"
        ))),
    }
}

/// Rows of one table and nothing else, with the pushed-down filter and the
/// join's narrowing.
fn rows_filter(
    table: Uuid,
    propf: Option<&Expr<PropertiesLiteral>>,
    key_hint: Option<&KeyHint>,
) -> EntityFilterAst {
    let mut rows = Expr::val(DatabaseRowLiteral::TableId(table));
    let mut properties = propf.cloned();
    match key_hint.and_then(narrowing) {
        Some(Narrowing::Rows(ids)) => rows = Expr::and(rows, ids),
        Some(Narrowing::Properties(values)) => {
            properties = Some(match properties {
                Some(properties) => Expr::and(properties, values),
                None => values,
            });
        }
        None => {}
    }
    EntityFilterAst {
        favorites_only: None,
        calendar_event_filter: Some(Arc::new(Expr::val(CalendarEventLiteral::Id(Uuid::nil())))),
        document_filter: Some(Arc::new(Expr::val(DocumentLiteral::Id(Uuid::nil())))),
        project_filter: Some(Arc::new(Expr::val(ProjectLiteral::ProjectIdSelf(
            Uuid::nil(),
        )))),
        chat_filter: Some(Arc::new(Expr::val(ChatLiteral::ChatId(Uuid::nil())))),
        email_filter: EmailFilterAst {
            tree: Some(Arc::new(Expr::val(EmailLiteral::ThreadId(Uuid::nil())))),
            crm_scope: None,
        },
        channel_filter: Some(Arc::new(Expr::val(ChannelLiteral::ChannelId(Uuid::nil())))),
        channel_thread_filter: Some(Arc::new(Expr::val(ChannelThreadLiteral::ThreadId(
            Uuid::nil(),
        )))),
        call_filter: Some(Arc::new(Expr::val(CallLiteral::CallId(Uuid::nil())))),
        crm_company_filter: Some(Arc::new(Expr::val(CrmCompanyLiteral::Id(Uuid::nil())))),
        foreign_entity_filter: Some(Arc::new(Expr::val(ForeignEntityLiteral::Id(Uuid::nil())))),
        // Reminders, agent sessions and initiatives are opt-in: left empty,
        // they are excluded.
        reminder_filter: None,
        agent_session_filter: None,
        initiative_filter: None,
        database_row_filter: Some(Arc::new(rows)),
        properties_filter: properties.map(Arc::new),
    }
}

enum Narrowing {
    Rows(Expr<DatabaseRowLiteral>),
    Properties(Expr<PropertiesLiteral>),
}

/// A filter fetching only the joined rows the join can match. The fold
/// applies the join regardless, so a hint that cannot be expressed fetches
/// the whole table instead.
fn narrowing(hint: &KeyHint) -> Option<Narrowing> {
    enum Member<'a> {
        Entity(&'a str),
        Option(Uuid),
    }
    let mut members = Vec::new();
    for value in &hint.values {
        match value {
            Cell::Entities(ids) => members.extend(ids.iter().map(|id| Member::Entity(id))),
            Cell::Options(ids) => members.extend(ids.iter().copied().map(Member::Option)),
            _ => return None,
        }
    }
    if members.is_empty() || members.len() > MAX_KEY_HINT_VALUES {
        return None;
    }
    match hint.column {
        None => {
            let ids = members
                .iter()
                .map(|member| match member {
                    Member::Entity(id) => Uuid::parse_str(id).ok(),
                    Member::Option(id) => Some(*id),
                })
                .collect::<Option<Vec<Uuid>>>()?;
            balanced_or(
                ids.into_iter()
                    .map(|id| Expr::val(DatabaseRowLiteral::Id(id)))
                    .collect(),
            )
            .map(Narrowing::Rows)
        }
        Some(column) => {
            let literals = members
                .iter()
                .map(|member| {
                    let value = match member {
                        Member::Entity(id) => {
                            PropertyMatchValue::EntityRef(EntityRefId::new((*id).to_owned()).ok()?)
                        }
                        Member::Option(id) => PropertyMatchValue::SelectOption(*id),
                    };
                    Some(Expr::val(PropertiesLiteral {
                        property_definition_id: column,
                        entity_type: None,
                        value,
                    }))
                })
                .collect::<Option<Vec<_>>>()?;
            balanced_or(literals).map(Narrowing::Properties)
        }
    }
}

/// A balanced OR keeps a long list shallow.
fn balanced_or<Literal>(mut items: Vec<Expr<Literal>>) -> Option<Expr<Literal>> {
    if items.len() < 2 {
        return items.pop();
    }
    let right = items.split_off(items.len() / 2);
    Some(Expr::or(balanced_or(items)?, balanced_or(right)?))
}

fn table_row(item: SoupItem<SoupPropertiesField>) -> Result<Row, SourceError> {
    let SoupItem::DatabaseRow(row) = item else {
        return Err(SourceError(format!(
            "a table query returned {:?}",
            item.entity()
        )));
    };
    Ok(Row {
        id: row.id,
        position: Some(row.position),
        cells: row
            .extra
            .properties
            .into_iter()
            .filter_map(|property| Some((property.definition.id, cell(property.value?))))
            .collect(),
    })
}

/// A property value as the engine reads it, matching the browser's source.
fn cell(value: PropertyValue) -> Cell {
    match value {
        PropertyValue::Bool(value) => Cell::Bool(value),
        PropertyValue::Num(value) => Cell::Number(value),
        PropertyValue::Str(value) => Cell::Text(value),
        PropertyValue::Date(value) => Cell::Date(value),
        PropertyValue::SelectOption(ids) => Cell::Options(ids),
        PropertyValue::EntityRef(references) => Cell::Entities(
            references
                .into_iter()
                .map(|reference| reference.entity_id)
                .collect(),
        ),
        PropertyValue::Link(urls) => Cell::Text(urls.join(" ")),
    }
}
