# Databases

Choose **Create → Database** to create a database and its first table with a Name
column. **C → B** opens a new database with its title selected and ready to type.
Enter saves the title and focuses A1, ready to type without another click.
Databases open at
`/app/database/<uuid>`. A database contains tables; records belong to a table and
its properties describe each record. The database name in the split header is an
inline input for editors: click it to rename. Enter or leaving it saves; Escape
cancels, and a failed rename shows a toast. Viewers without edit access see the
split header's `viewer` badge instead.

## Properties and records

Use **Add column** immediately after the table’s headers. It creates an **Unnamed**
Text column (**Unnamed 2**, and so on if that name exists), selects its name in the
header, and lets you type immediately. Enter saves; Escape keeps the default name.
There is no creation dialog. Double-click a header, press F2 while it is focused,
or choose **Rename column** from its arrow or right-click menu to rename it later.
Its type icon stays in place while editing. SQL refers to tables and columns by
their display names (double-quoted), so a rename changes the name a saved query
must use.

The header arrow menu groups schema and view actions. **Change type** offers Text,
Number, Select, Multi-select, Date, Checkbox, URL, People, Documents, Tasks, and
relations to tables in this database. Opening it checks the column's values
against every type. A type no value can become is greyed out, with the reason
under its name (a checkbox can only become text; only an empty column can become
People or a relation). A type some values would not survive shows how many, such
as "3 values aren't numbers"; choosing it opens a confirmation listing a few of
them, and **Convert anyway, clearing 3 values** converts the rest and empties
those (a cell with several values keeps its first). Every other type converts
immediately. Plain number strings can become numbers; padding, leading zeros and
ambiguous values count as values that don't fit. A date becomes its `YYYY-MM-DD`
text. Changing a placement never changes another table that uses the same property.
A relation can hold multiple records. **Delete column** opens a confirmation;
it removes this table’s column and values while preserving other tables.
Drag a column header left or right to reorder it, or use **Move left / Move right**.
To add a column next to another, right-click its header and choose **Insert left** or
**Insert right**: a Text column appears on that side with its name selected for
editing, and its type is inferred from what you type.
An orange insertion line shows the exact boundary before or after the target
column. Release to place it there; Escape cancels. Original-position and
offscreen boundaries show no line and do not change the order.
The pointer can pass over the rows while reordering. Hold it at the grid's left
or right edge to scroll to other columns. Drops move immediately while saves are
queued, so another drag does not need to wait for the previous save.

The first nonempty entry in a new default Text column sets its type. A plain number
becomes Number; starting with `@` and choosing an item makes it the corresponding
reference type. Other entries keep Text, and identifiers with leading zeros stay
text. Choosing Text explicitly disables inference. Populated columns never infer a
new type. Reference cells use Macro’s native mention menu, limited to the selected
kind (for example, People shows users, Tasks shows tasks). Arrow keys navigate and
Enter chooses; Escape returns without changing the value. Delete clears a selected
reference cell. Text supports markdown and inline native mentions such as
`Say hi to @Maya`, stored using the same mention encoding as documents. A mention
inside a sentence preserves Text. Select values offer **Add option** for new choices.
Multi-select menus toggle each option independently and keep the other selections.

**New table**, beside the table tabs in the toolbar under the split header, creates another table
with a Name column. Enter a table name and press Enter. If the table is created
but its column setup fails,
**Retry setup** continues that same table; **Open table** lets you finish manually.
To rename a table, right-click its tab and choose **Rename table**, double-click
the tab, or focus it and press F2. These actions also work on inactive tabs.
The tab itself becomes an input. Enter or leaving the input saves; Escape cancels.
Renaming preserves records and saved views. A concurrent rename asks you to reopen
the editor, and a failed request keeps your draft available to retry.
To reorder tables, drag a tab along the tab strip; an accent line shows where it
will land, and Escape cancels the drag. From the keyboard, focus a tab, press
Shift+F10 (or right-click it) and choose **Move left** or **Move right**; each is
disabled at its end of the strip. The new order shows at once, is saved for every
viewer, and survives a reload. If the save fails, the tabs return to their previous
order and a toast says so.

An editable empty row always follows the records. Enter a value in any of its
cells to create a record; the next empty row appears immediately. Merely focusing
or tabbing through an empty row does not save an empty record.
Click a cell to edit, or focus it with the arrow keys and start typing. Enter
saves and keeps that cell selected; Down then selects the same column in the
next row, ready to type. Arrow keys inside a text editor keep their native cursor
behavior. Escape cancels. Tab saves and immediately edits the next writable cell;
Shift+Tab moves backward, and both wrap between rows. Read-only columns are
skipped. Select values open a menu, including **Add option**; checkboxes change
directly. Type on a selected select cell to search its options; Enter chooses a
match, and Tab chooses the focused option or typed match before moving on.
Date cells open Macro's date selector (the one tasks use): type a date or phrase
such as `tomorrow`, `3d`, or `feb 17` and press Enter, or pick **Custom date...**
for a calendar. Typing on a selected date cell starts that search; Delete clears
the date, and Tab leaves the selector without changing it.
Invalid numbers remain in the editor for correction.
Arrow keys also move between checkbox and closed select cells without changing
their values or opening a menu. Enter opens a selected select cell's menu.
Blank grid lines continue below the editable row to fill the available space.
Rapid edits remain attached to the same row while its first save is in flight.
Committed cell edits continue saving to their original table if you switch tables
while that first save is in flight.

Right-click a cell or row number for **Edit cell**, **Open record**, **Rename**,
**Duplicate**, or **Delete record**, as applicable. Shift+F10 or the keyboard menu
key opens the same menu for a focused cell. Right-clicking an active text input
keeps its native copy/paste menu. Duplicate copies editable values into a new row
and focuses its name. Deletion requires confirmation.

Click a row number or choose **Open record** for a compact centered dialog. The
name is editable once at the top, with the remaining properties below. Tab moves
directly between editable fields; previous/next controls browse the current view.
Close or Escape returns to the grid.

Relation cells show the names of records in their related table. Click one or
start typing to search that table, then select records to link them. The selected
chips open the referenced record in the compact editor; their **Remove** buttons
remove only the relationship, never the related record. Enter selects a match.
Tab selects a searched match before moving to the next cell; with an empty search
it only moves. Escape closes the picker and returns focus to the cell. Choosing a
relation in the empty row creates the record and its links together. View-only
users can open references but cannot select or remove them. Relation cells show
current record names, and unavailable records have a readable label. Search and
sorting skip relations; a relation filter can only test whether it is empty.

Edits save automatically. A failed save appears as an actionable error above the grid.
A concurrent edit can cause a version conflict: the latest values load and
**Retry** reapplies the rejected change. A failed refresh after a successful save
offers a refresh action; creating the record again would create a duplicate.
If a row's create response is lost, its draft remains and the notice says it may
already be saved. **Refresh** only reads the latest rows. Compare them with the
draft, then use **Discard draft** to remove the local draft without deleting any
saved record. The same uncertain draft cannot submit another insert.
View-only access allows browsing and personal view controls but disables data
and schema changes.

## Table and board views

Tables contain the records; the **Views** beside **All records** are saved ways to
show those same records. **New view** offers Table and Board. A Board requires a
Select, Multi-select, or Checkbox property; choose any compatible property in
**Group by**. The choice is based on the table's schema, without a special Status
property. **View settings** can change the current layout or grouping later.
Switching to Board there without a grouping picks the first Multi-select, else the
first Select, else the first Checkbox; an existing grouping is kept. Its
**Columns** list shows or hides each property with a switch.
If the table has no grouping property, **Open table** returns to its grid so you
can create a column and choose a suitable type from its header menu.

Lanes start in alphabetical order, including empty options and a **No …** lane
for unassigned records. Drag a lane header to reorder lanes, or focus its handle
and press Alt+Left / Alt+Right. Saved views remember the lane order automatically.
An orange line marks the lane's before/after insertion boundary. For cards, the
line follows the gap under the pointer, including gaps within the current lane.
Release to place the card exactly there. Dragging switches a sorted board to
manual order without moving its other cards. Saved views remember card positions.
Escape cancels a drag; releasing outside the board leaves the record unchanged.
The space beneath a lane's cards also accepts drops. Hold near the board's edge
to scroll while dragging.
Drag anywhere on a card into another lane, keeping its original size and shape.
The card's **Move …** menu offers the same action without dragging. On a multi-select
board a card can appear in several lanes: moving it replaces that lane's value
and keeps its other selections; moving it to the unassigned lane clears them.

**+ New** at the bottom of a lane (or the lane header's **+**) puts an empty,
focused card title in that lane; nothing opens. Enter creates the card with the
lane's value and opens another empty card below it, so several can be typed in a
row; each appears in place while it saves. Shift+Enter creates the card and opens
its record. Escape, or leaving an empty title, cancels; leaving a typed title
saves it. From the keyboard, press **n** with focus on any card or control in a
lane, or Enter on a focused lane header, to start a card there. The toolbar's
**New** starts one in the first lane. Failed requests keep the typed draft for
correction or retry; Enter in it retries. Open a card to edit its details. **New group** at the end of a select board adds
another option and lane. Every new select option, a new group included, takes
the next colour of the tag palette, so its pill is coloured wherever it shows. Hiding a property does not change the record's title.
The table grid uses its always-ready empty row instead of a separate New button.

**Filter**, **Sort**, **Search**, and **View settings** are grouped at the right of
the views row. Boards also offer **New**.
**Search** expands an inline **Search records** field; **Clear search** leaves
filters intact and keeps that field focused. Escape clears and closes search.
In **Filter**, the first condition reads **Where** and each later one has an
**And**/**Or** control; the choice applies to every condition at once. A
condition with no value yet is ignored. Select and multi-select values are offered
as the same colored pills the cells show, behind a **Choose** placeholder.
Multi-select filters match the selected members. Search, filters, and sorting run
in the database engine as one SQL statement, so text matches ignore case and dates
compare by calendar day (UTC). Column headers offer sorting, **Move left**, **Move right**, and
**Hide column** for the current view. Moves skip hidden columns; the Columns
switches in **View settings** restore a hidden column to its saved position.
Hidden columns remain available in the record dialog, and new columns appear
after the saved layout.
Creating or changing a record can make it fall outside the current search or
filters. A saved-record notice offers **Open record** to inspect it without
changing the view. Its record dialog explains why it is outside the view; you can
continue editing there. A failed data refresh after creation keeps the saved
record available rather than requiring another create.
**New view** offers Table or Board, a name, and **Create view**. It stores the
current filters, sorting, column order, and visible columns as a personal view
for this table.
Saved views appear beside **All records**. Right-click a saved view for **Rename
view** or **Delete view**, or double-click/F2 to rename it directly in its tab.
Enter saves and Escape cancels. These actions target the clicked view, even when
another view is selected. **Save as new view** copies
the active view. Shift+F10 or the keyboard menu key opens tab context menus.
Use **Save changes** to update an edited saved view. The selected table and saved
view, including unsaved view adjustments, are restored when reopening the database.

## First database

When Databases is enabled and an authenticated user has no accessible databases,
the app creates one small **Getting started** example in the background. Its
**Ideas** table has Name and Stage columns and three cards spread across To do,
Doing, and Done. The saved Table and Board views show the same records; the first
open selects Board. This example is created at most once per user. Retrying or
opening another tab never overwrites edits, and removing the example does not
cause it to reappear. The app waits for the feature flag and database list before
provisioning, and disabled users receive no starter database.

## AI questions and live answers

Open **AI** (`Database AI`) at the right of the toolbar to create a native chat in the adjacent split.
Its bottom composer contains a database mention and private context identifying
this database, its current table, and all its tables. Nothing sends automatically.
Type a question or requested change and send it using the normal chat controls.
Any chat, not only one opened from a database, can build databases: the assistant
has `ListDatabases`, `DescribeDatabase`, `QueryDatabase`, `CreateDatabase`,
`CreateTable`, `RenameTable`, `ReorderTables`, `AddColumn` (including relation columns via `linkToTableId`),
`AddColumnOptions`, and `SaveDatabaseView`. It reads current schema before editing
and checks actual results before reporting success.

Query tool results render inline. Their display menu switches between a table,
a scalar answer, or compatible bar, line, and pie charts. A saved-view tool result
offers **Open view**, which opens that database/table and selects the created view.
The same tools are exposed to agent sessions through the Macro MCP server.

In a document, `/database` → **Database** opens the question box with the AI prompt focused
immediately. The empty input rotates through example questions; a selected database
uses its actual table and column names. Typing hides these hints, and reduced-motion
preferences keep them static. Use the searchable source picker beside **SQL** to
choose a database; the entire chosen database is in scope, without a table
prerequisite. **Automatic** finds a relevant accessible database from the question
with read-only discovery tools and inspects all its tables; if matching sources are
ambiguous, the assistant asks for clarification. Type to search the
source menu, use the arrow keys and Enter to choose, or Escape to return without
changing it. The displayed source is checked against the query's actual table
dependencies. Single values default to an inline answer; multiple records become a
result table. Asking for a chart can produce a **Bar chart**, **Line chart**, or
**Pie chart**. The answer's display menu offers the formats supported by its data;
**View data** opens the underlying result table. Missing values remain empty, and
an unavailable chart falls back to the table with an explanation. Charts copied
into documents stay live: only their query and chart settings are saved, never a
copy of the reader's results. **Insert answer** saves a new answer; **Save changes** updates an
existing one. Existing answers retain their resolved source and preview their
saved query when opened. Changing the question or source preserves the draft but
requires updating the result before saving. Results refresh
when their source tables change, and each reader sees only data they can access.
The AI supplies a short answer title independently of the original question.
Double-click that title (or focus it and press F2) to rename it inline; Enter saves,
Escape cancels. Renaming keeps the question and SQL unchanged. Table references
survive database and table renames.

Database creation, navigation, slash actions, and interactive answer chips are
controlled by the `enable-databases` feature flag (`VITE_ENABLE_DATABASES` locally).
An existing document keeps its answer label when the flag is off and does not fetch
its database results.

## Sharing and files

**Share** opens Macro's standard sharing dialog. The owner can share with people
or channels and change or remove their access. Databases do not offer a public
link. **Share** and viewer avatars (other people currently looking at the
database) sit at the right of the split header, like other entities.

The toolbar's **Database actions** (`…`) menu, beside **AI**, contains **Rename**,
**Import CSV**, **Download**, and owner-only **Delete**. Rename focuses the inline
title in the split header; Delete opens the standard confirmation dialog.
**Download** offers **Current table as CSV**.
Exports contain all records, regardless of the current filters. CSV uses column
labels and preserves text, quoted commas, and line breaks. A table that changes
during CSV export must be downloaded again so the file does not mix different
versions.

**Import CSV** in the `…` menu opens a preview and focuses the new table's name.
Confirm **Import** to create a table with the CSV's columns and records. Imported
values remain Text, preserving leading zeros and large identifiers; use a column's
type menu afterward to convert it. The limit is 8 MB, 100 columns, and 10,000 rows.
A failed response offers **Retry import** with the same request, so retrying a
completed import does not create a second table.

## Side panel and activity

The split header's `Show Side Panel` toggle (or `]`) opens the database side panel,
closed by default. **Details** shows the Owner and Created time. **Activity** (behind
the `enable-entity-activity-section` flag, like documents) lists who created,
renamed, edited, shared, trashed, or restored the database, with the same glyph rail
and folding as a document's Activity section. Every write is one entry: a cell edit,
a SQL statement however many rows it touches, or a schema change such as a new
table or column. Consecutive edits by one person fold into `made N edits`. Changes an
AI agent made read as the agent acting for the user who asked. The same entries
appear on `/app/component/activity`; clicking one opens the database.
