-- One branch per sort, each reading its index in sort order and stopping after
-- $3 rows. Branch conditions on parameters alone are checked once per
-- execution, so only the requested sort's branches run, also under a cached
-- generic plan. A single ORDER BY CASE $4 reads and sorts the whole team.
--
-- Every branch also returns the user's view time. The viewed branch has it from
-- the history row it reads and the unviewed branches know it is NULL, so the
-- final select looks up no history per company.
WITH limited_companies AS (
    SELECT *
    FROM (
        -- Explicit ids are few, so this branch has no index order or limit.
        (
            SELECT
                c.id, c.team_id, c.custom_name, c.email_sync, c.hidden,
                c.first_interaction, c.last_interaction,
                CASE $4
                    WHEN 'created_at' THEN c.first_interaction
                    WHEN 'viewed_at' THEN uh."updatedAt"
                    WHEN 'viewed_updated'
                        THEN COALESCE(uh."updatedAt", c.last_interaction)
                    ELSE c.last_interaction
                END AS sort_ts,
                uh."updatedAt"::timestamptz AS viewed_at
            FROM crm_companies c
            LEFT JOIN "UserHistory" uh
                ON uh."itemId" = c.id::text
               AND uh."itemType" = 'crm_company'
               AND uh."userId" = $8
            WHERE cardinality($2::uuid[]) > 0
              AND c.id = ANY($2::uuid[])
              AND c.team_id = $1
              AND c.hidden = COALESCE($5::bool, FALSE)
        )
        UNION ALL
        -- The seek bound repeats the keyset's leading column so the index
        -- scan starts at the cursor. Both interaction columns are NOT NULL,
        -- so it drops no rows; the row comparison stays exact.
        (
            SELECT
                c.id, c.team_id, c.custom_name, c.email_sync, c.hidden,
                c.first_interaction, c.last_interaction,
                c.last_interaction,
                uh."updatedAt"::timestamptz
            FROM crm_companies c
            LEFT JOIN "UserHistory" uh
                ON uh."itemId" = c.id::text
               AND uh."itemType" = 'crm_company'
               AND uh."userId" = $8
            WHERE cardinality($2::uuid[]) = 0
              AND $4 = 'updated_at'
              AND c.team_id = $1
              AND c.hidden = COALESCE($5::bool, FALSE)
              AND c.last_interaction <= COALESCE($6::timestamptz, 'infinity')
              AND ($6::timestamptz IS NULL OR (c.last_interaction, c.id::text) < ($6, $7))
            ORDER BY c.last_interaction DESC NULLS LAST, c.id DESC
            LIMIT $3
        )
        UNION ALL
        (
            SELECT
                c.id, c.team_id, c.custom_name, c.email_sync, c.hidden,
                c.first_interaction, c.last_interaction,
                c.first_interaction,
                uh."updatedAt"::timestamptz
            FROM crm_companies c
            LEFT JOIN "UserHistory" uh
                ON uh."itemId" = c.id::text
               AND uh."itemType" = 'crm_company'
               AND uh."userId" = $8
            WHERE cardinality($2::uuid[]) = 0
              AND $4 = 'created_at'
              AND c.team_id = $1
              AND c.hidden = COALESCE($5::bool, FALSE)
              AND c.first_interaction <= COALESCE($6::timestamptz, 'infinity')
              AND ($6::timestamptz IS NULL OR (c.first_interaction, c.id::text) < ($6, $7))
            ORDER BY c.first_interaction DESC NULLS LAST, c.id DESC
            LIMIT $3
        )
        UNION ALL
        -- Companies the user has viewed, for both viewed sorts. Bounded by the
        -- size of the user's company history rather than the team.
        (
            SELECT
                c.id, c.team_id, c.custom_name, c.email_sync, c.hidden,
                c.first_interaction, c.last_interaction,
                uh."updatedAt"::timestamptz AS sort_ts,
                uh."updatedAt"::timestamptz
            FROM "UserHistory" uh
            JOIN crm_companies c ON c.id::text = uh."itemId"
            WHERE cardinality($2::uuid[]) = 0
              AND $4 IN ('viewed_at', 'viewed_updated')
              AND uh."userId" = $8
              AND uh."itemType" = 'crm_company'
              AND c.team_id = $1
              AND c.hidden = COALESCE($5::bool, FALSE)
              AND ($6::timestamptz IS NULL OR (uh."updatedAt"::timestamptz, c.id::text) < ($6, $7))
            ORDER BY sort_ts DESC NULLS LAST, c.id DESC
            LIMIT $3
        )
        UNION ALL
        -- Unviewed companies sort as NULL under viewed_at. NULL never passes a
        -- cursor, so they only appear on a first page.
        --
        -- NOT IN rather than NOT EXISTS in both unviewed branches: "itemId" is
        -- NOT NULL, so they are equivalent, and NOT IN is planned as one hashed
        -- lookup of the user's history instead of a scan of it per company.
        (
            SELECT
                c.id, c.team_id, c.custom_name, c.email_sync, c.hidden,
                c.first_interaction, c.last_interaction,
                NULL::timestamptz,
                NULL::timestamptz
            FROM crm_companies c
            WHERE cardinality($2::uuid[]) = 0
              AND $4 = 'viewed_at'
              AND $6::timestamptz IS NULL
              AND c.team_id = $1
              AND c.hidden = COALESCE($5::bool, FALSE)
              AND c.id::text NOT IN (
                  SELECT uh."itemId" FROM "UserHistory" uh
                  WHERE uh."userId" = $8
                    AND uh."itemType" = 'crm_company'
              )
            ORDER BY c.id DESC
            LIMIT $3
        )
        UNION ALL
        -- Unviewed companies under viewed_updated fall back to last_interaction.
        (
            SELECT
                c.id, c.team_id, c.custom_name, c.email_sync, c.hidden,
                c.first_interaction, c.last_interaction,
                c.last_interaction,
                NULL::timestamptz
            FROM crm_companies c
            WHERE cardinality($2::uuid[]) = 0
              AND $4 = 'viewed_updated'
              AND c.team_id = $1
              AND c.hidden = COALESCE($5::bool, FALSE)
              AND c.last_interaction <= COALESCE($6::timestamptz, 'infinity')
              AND ($6::timestamptz IS NULL OR (c.last_interaction, c.id::text) < ($6, $7))
              AND c.id::text NOT IN (
                  SELECT uh."itemId" FROM "UserHistory" uh
                  WHERE uh."userId" = $8
                    AND uh."itemType" = 'crm_company'
              )
            ORDER BY c.last_interaction DESC NULLS LAST, c.id DESC
            LIMIT $3
        )
    ) candidates
    WHERE EXISTS (
        SELECT 1 FROM team_crm_settings tcs
        WHERE tcs.team_id = $1 AND tcs.crm_enabled
    )
      -- Keyset seek (NULL = first page): keep only rows that sort strictly
      -- after the cursor.
      AND ($6::timestamptz IS NULL OR (sort_ts, id::text) < ($6, $7))
    ORDER BY sort_ts DESC NULLS LAST, id DESC
    LIMIT $3
)
SELECT
    lc.id                AS "company_id!",
    lc.team_id           AS "company_team_id!",
    lc.email_sync        AS "company_email_sync!",
    lc.hidden            AS "company_hidden!",
    lc.first_interaction AS "company_created_at!",
    lc.last_interaction  AS "company_updated_at!",
    d.id                 AS "domain_id?",
    d.domain             AS "domain?",
    d.created_at         AS "domain_created_at?",
    COALESCE(lc.custom_name, dd.name) AS "display_name?",
    dd.description       AS "dir_description?",
    lc.viewed_at         AS "viewed_at?"
FROM limited_companies lc
LEFT JOIN crm_domains d ON d.company_id = lc.id
LEFT JOIN crm_domain_directory dd
    ON LOWER(dd.domain) = LOWER(d.domain)
ORDER BY
    lc.sort_ts DESC NULLS LAST,
    lc.id DESC,
    d.created_at ASC NULLS LAST
