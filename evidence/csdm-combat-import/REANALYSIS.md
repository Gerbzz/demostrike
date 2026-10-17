# Annotation-preserving reanalysis boundary

Schema v7 stores player-round aggregates; v8 additionally keeps ordered raw
combat facts. Both are inserted with the match in one transaction. Older
libraries migrate additively. A migration must not derive combat rows from
utility-only data or replace a match, event, favorite, tag, collection, or
render-job link. The v6 migration test carries all of those references through
v7 and v8. Cancellation before commit rolls back match and raw facts together.

Ordinary duplicate imports remain blocked by canonical path and source hash.
Future reanalysis of a known source should be a separate, explicit operation:

1. Verify the existing source checksum against a private snapshot. Keep the
   original match ID, utility event IDs, and annotation tables intact.
2. Parse into temporary facts, validate coverage and capability, then derive
   new versioned combat rows outside the live match transaction.
3. In one transaction, verify the match checksum has not changed, replace only
   rows in versioned combat analysis tables for that match, update capability
   after the rows pass validation, and commit. A failure or cancellation leaves
   the previous rows and capability untouched.
4. If utility event identity ever changes, require an explicit reviewed mapping
   from old event IDs to new event IDs before altering utility rows or their
   favorite/tag/collection/render links. No such mapping is implemented here.

No automatic or destructive reimport is part of this milestone. Preserving the
raw event stream in v8 makes a later analysis upgrade possible, but full CSDM
team/economy/rating calculations still require their own verified inputs.
