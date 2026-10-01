//! A board view's rows as cards in lanes.

#[cfg(test)]
mod test;

use models_databases::position::Position;
use models_databases::views::{CardPosition, DatabaseView, ViewLayout, ViewProblem, arrange_lane};
use models_databases::{OptionId, RowId};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::catalog::{Catalog, ColumnKind};
use crate::fold::Cell;
use crate::run::Outcome;

use super::{checked_table, placed};

/// A board's lanes in display order, every lane included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    /// The lanes, in display order.
    pub lanes: Vec<BoardLane>,
}

/// One lane and its cards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoardLane {
    /// The option whose cards the lane holds; `null` for cards without one.
    pub option: Option<OptionId>,
    /// Whether the lane is hidden: by the layout, or for being empty.
    pub hidden: bool,
    /// The cards' rows, in display order.
    pub cards: Vec<RowId>,
}

/// Lay out the rows `outcome` holds (what running
/// [`compile_view`](super::compile_view)'s query produced) as the view's
/// board, ordering unsorted lanes by the stored `positions`.
pub fn board(
    view: &DatabaseView,
    catalog: &Catalog,
    outcome: &Outcome,
    positions: &[CardPosition],
) -> Result<Board, ViewProblem> {
    let table = checked_table(view, catalog)?;
    let ViewLayout::Board {
        group_by,
        lanes: listed,
        hide_empty_lanes,
        ..
    } = &view.layout
    else {
        return Err(ViewProblem::NotABoard);
    };
    let grouping = placed(table, *group_by);
    let ColumnKind::Select { options, .. } = &grouping.kind else {
        unreachable!("the view check makes a board's column a single select");
    };
    let cell_index = outcome
        .columns
        .iter()
        .position(|column| column.column == Some(grouping.id))
        .ok_or(ViewProblem::UnknownColumn { column: *group_by })?;

    let mut order: Vec<Option<OptionId>> = listed.iter().map(|lane| lane.option).collect();
    let unlisted = std::iter::once(None)
        .chain(options.iter().map(|option| Some(option.id)))
        .filter(|lane| !order.contains(lane))
        .collect::<Vec<_>>();
    order.extend(unlisted);

    let mut cards: Vec<Vec<RowId>> = vec![Vec::new(); order.len()];
    for (row, cells) in outcome.row_ids.iter().zip(&outcome.rows) {
        let lane = match cells.get(cell_index) {
            Some(Some(Cell::Options(ids))) => match ids.as_slice() {
                [only] => order.iter().position(|lane| *lane == Some(*only)),
                _ => None,
            },
            _ => None,
        }
        .unwrap_or_else(|| {
            order
                .iter()
                .position(Option::is_none)
                .expect("every board has the lane of cards without an option")
        });
        cards[lane].push(*row);
    }

    let sorted = !view.query.sort.is_empty();
    let lanes = order
        .into_iter()
        .zip(cards)
        .map(|(option, cards)| {
            let cards = if sorted {
                cards
            } else {
                arranged(option, cards, positions)
            };
            let hidden_by_layout = listed
                .iter()
                .any(|lane| lane.option == option && lane.hidden);
            BoardLane {
                option,
                hidden: hidden_by_layout || (*hide_empty_lanes && cards.is_empty()),
                cards,
            }
        })
        .collect();
    Ok(Board { lanes })
}

/// One lane's cards in hand-arranged order. A stored position counts only
/// in the lane it was stored for: a card whose cell has since changed has no
/// place in its new lane yet.
fn arranged(lane: Option<OptionId>, cards: Vec<RowId>, positions: &[CardPosition]) -> Vec<RowId> {
    let mut placed: Vec<(RowId, Option<Position>)> = cards
        .into_iter()
        .map(|row| {
            let position = positions
                .iter()
                .find(|stored| stored.row == row && stored.lane == lane)
                .map(|stored| stored.position.clone());
            (row, position)
        })
        .collect();
    arrange_lane(&mut placed);
    placed.into_iter().map(|(row, _)| row).collect()
}
