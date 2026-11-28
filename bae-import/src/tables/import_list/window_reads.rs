//! The import list's window as one compiled read for each order the list
//! shows. Each is the join of the rows that others sort under (`u`) with the
//! rows sorting under each (`w`), read from the window's first key on in the
//! order's indexes, so coven reruns a full window only for a write at or
//! before its last row. A window from the list's top reads from the order's
//! first row through its own statement (`top_reads`). Each row selects its key, its release's placement, what
//! the placement's fragments read past it, and for a member of a group,
//! whether the list shows no member of the group after it.

// The window of the list newest first.
sqlive_compose::read!(
    writer NewestFirstWindow(
        tab: String,
        filter: Option<String>,
        filter_text: Option<String>,
        folds_only: bool,
        folded: String,
        limit: i64,
        order_key_0: bae_store::list_windows::SortValue,
        order_key_1: bae_store::list_windows::SortValue,
        order_key_2: bae_store::list_windows::SortValue,
        order_key_3: bae_store::list_windows::SortValue,
        order_key_4: bae_store::list_windows::SortValue,
        order_key_5: bae_store::list_windows::SortValue,
        order_key_6: bae_store::list_windows::SortValue,
        order_key_7: bae_store::list_windows::SortValue,
        order_key_8: bae_store::list_windows::SortValue,
        order_key_9: bae_store::list_windows::SortValue,
        order_key_10: bae_store::list_windows::SortValue,
        order_key_11: bae_store::list_windows::SortValue,
        order_key_12: bae_store::list_windows::SortValue,
        order_key_13: bae_store::list_windows::SortValue
    ) {
        release_key: Option<crate::CandidateKey>, has_summary: Option<bool>,
        import_failed: Option<bool>, actionable: Option<bool>, ends_group: Option<bool>
    },
    r#"
    SELECT u.sort_queued AS order_key_0,
           u.sort_queue AS order_key_1,
           u.sort_undated AS order_key_2,
           u.sort_newest AS order_key_3,
           u.root_position AS order_key_4,
           u.watched_folder_path AS order_key_5,
           u.path_text AS order_key_6,
           u.entry AS order_key_7,
           w.sort_member AS order_key_8,
           w.sort_undated AS order_key_9,
           w.sort_newest AS order_key_10,
           w.path_text AS order_key_11,
           w.key AS order_key_12,
           w.entry AS order_key_13,
           w.item,
           p.watched_folder_path AS release_root, p.key AS release_key, p.has_summary,
           p.shown_title, p.shown_credits, p.tab, p.import_failed, p.actionable,
           p.pending_state, p.needs_you_reason, p.reason_pressings, p.error_detail,
           @crate::tables::import_list::row_fragments::ListedActions(parent: p) AS actions,
           @crate::tables::import_list::row_fragments::ListedRow(parent: p) AS listed,
           @crate::tables::import_list::row_fragments::ListedHeader(
               filter: :filter, filter_text: :filter_text, folds_only: :folds_only,
               folded: :folded, parent: w) AS header,
           CASE WHEN w.item <> 'group' AND w.group_folder IS NOT NULL THEN NOT EXISTS (
               SELECT 1 FROM import_list_placement AS m
               WHERE m.unit_entry = w.unit_entry
                 AND (m.sort_member, m.sort_undated, m.sort_newest, m.path_text,
                    m.key, m.entry)
                     > (w.sort_member, w.sort_undated, w.sort_newest, w.path_text,
                    w.key, w.entry)
                 AND @crate::tables::import_list::row_fragments::Kept(
                     filter: :filter, filter_text: :filter_text, parent: m)) END AS ends_group
    FROM import_list_placement AS u
    CROSS JOIN import_list_placement AS w
        ON w.unit_entry = u.entry
       AND w.sort_member <= CASE WHEN u.item = 'group' AND @crate::tables::import_list::row_fragments::Folded(
              folds_only: :folds_only, folded: :folded, parent: u) THEN 0 ELSE 1 END
    LEFT JOIN import_list_placement AS p ON p.entry = w.entry AND w.item <> 'group'
    WHERE u.tab = :tab AND u.unit_entry = u.entry
      AND @crate::tables::import_list::row_fragments::Kept(
          filter: :filter, filter_text: :filter_text, parent: w)
      AND (u.sort_queued, u.sort_queue, u.sort_undated,
           u.sort_newest, u.root_position, u.watched_folder_path,
           u.path_text, u.entry)
          >= (:order_key_0, :order_key_1, :order_key_2,
              :order_key_3, :order_key_4, :order_key_5,
              :order_key_6, :order_key_7)
      AND (w.sort_member, w.sort_undated, w.sort_newest, w.path_text,
           w.key, w.entry)
          >= (
             CASE WHEN (u.sort_queued, u.sort_queue, u.sort_undated,
                 u.sort_newest, u.root_position, u.watched_folder_path,
                 u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5,
                    :order_key_6, :order_key_7)
             THEN :order_key_8 ELSE -1 END,
             CASE WHEN (u.sort_queued, u.sort_queue, u.sort_undated,
                 u.sort_newest, u.root_position, u.watched_folder_path,
                 u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5,
                    :order_key_6, :order_key_7)
             THEN :order_key_9 ELSE 0 END,
             CASE WHEN (u.sort_queued, u.sort_queue, u.sort_undated,
                 u.sort_newest, u.root_position, u.watched_folder_path,
                 u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5,
                    :order_key_6, :order_key_7)
             THEN :order_key_10 ELSE 0 END,
             CASE WHEN (u.sort_queued, u.sort_queue, u.sort_undated,
                 u.sort_newest, u.root_position, u.watched_folder_path,
                 u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5,
                    :order_key_6, :order_key_7)
             THEN :order_key_11 ELSE 0 END,
             CASE WHEN (u.sort_queued, u.sort_queue, u.sort_undated,
                 u.sort_newest, u.root_position, u.watched_folder_path,
                 u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5,
                    :order_key_6, :order_key_7)
             THEN :order_key_12 ELSE 0 END,
             CASE WHEN (u.sort_queued, u.sort_queue, u.sort_undated,
                 u.sort_newest, u.root_position, u.watched_folder_path,
                 u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5,
                    :order_key_6, :order_key_7)
             THEN :order_key_13 ELSE 0 END)
    ORDER BY u.sort_queued, u.sort_queue, u.sort_undated,
             u.sort_newest, u.root_position, u.watched_folder_path,
             u.path_text, u.entry, w.sort_member,
             w.sort_undated, w.sort_newest, w.path_text,
             w.key, w.entry
    LIMIT :limit
"#
);

// The window of the list oldest first.
sqlive_compose::read!(
    writer OldestFirstWindow(
        tab: String,
        filter: Option<String>,
        filter_text: Option<String>,
        folds_only: bool,
        folded: String,
        limit: i64,
        order_key_0: bae_store::list_windows::SortValue,
        order_key_1: bae_store::list_windows::SortValue,
        order_key_2: bae_store::list_windows::SortValue,
        order_key_3: bae_store::list_windows::SortValue,
        order_key_4: bae_store::list_windows::SortValue,
        order_key_5: bae_store::list_windows::SortValue,
        order_key_6: bae_store::list_windows::SortValue,
        order_key_7: bae_store::list_windows::SortValue,
        order_key_8: bae_store::list_windows::SortValue,
        order_key_9: bae_store::list_windows::SortValue,
        order_key_10: bae_store::list_windows::SortValue,
        order_key_11: bae_store::list_windows::SortValue,
        order_key_12: bae_store::list_windows::SortValue,
        order_key_13: bae_store::list_windows::SortValue
    ) {
        release_key: Option<crate::CandidateKey>, has_summary: Option<bool>,
        import_failed: Option<bool>, actionable: Option<bool>, ends_group: Option<bool>
    },
    r#"
    SELECT u.sort_queued AS order_key_0,
           u.sort_queue AS order_key_1,
           u.sort_undated AS order_key_2,
           u.sort_oldest AS order_key_3,
           u.root_position AS order_key_4,
           u.watched_folder_path AS order_key_5,
           u.path_text AS order_key_6,
           u.entry AS order_key_7,
           w.sort_member AS order_key_8,
           w.sort_undated AS order_key_9,
           w.sort_oldest AS order_key_10,
           w.path_text AS order_key_11,
           w.key AS order_key_12,
           w.entry AS order_key_13,
           w.item,
           p.watched_folder_path AS release_root, p.key AS release_key, p.has_summary,
           p.shown_title, p.shown_credits, p.tab, p.import_failed, p.actionable,
           p.pending_state, p.needs_you_reason, p.reason_pressings, p.error_detail,
           @crate::tables::import_list::row_fragments::ListedActions(parent: p) AS actions,
           @crate::tables::import_list::row_fragments::ListedRow(parent: p) AS listed,
           @crate::tables::import_list::row_fragments::ListedHeader(
               filter: :filter, filter_text: :filter_text, folds_only: :folds_only,
               folded: :folded, parent: w) AS header,
           CASE WHEN w.item <> 'group' AND w.group_folder IS NOT NULL THEN NOT EXISTS (
               SELECT 1 FROM import_list_placement AS m
               WHERE m.unit_entry = w.unit_entry
                 AND (m.sort_member, m.sort_undated, m.sort_oldest, m.path_text,
                    m.key, m.entry)
                     > (w.sort_member, w.sort_undated, w.sort_oldest, w.path_text,
                    w.key, w.entry)
                 AND @crate::tables::import_list::row_fragments::Kept(
                     filter: :filter, filter_text: :filter_text, parent: m)) END AS ends_group
    FROM import_list_placement AS u
    CROSS JOIN import_list_placement AS w
        ON w.unit_entry = u.entry
       AND w.sort_member <= CASE WHEN u.item = 'group' AND @crate::tables::import_list::row_fragments::Folded(
              folds_only: :folds_only, folded: :folded, parent: u) THEN 0 ELSE 1 END
    LEFT JOIN import_list_placement AS p ON p.entry = w.entry AND w.item <> 'group'
    WHERE u.tab = :tab AND u.unit_entry = u.entry
      AND @crate::tables::import_list::row_fragments::Kept(
          filter: :filter, filter_text: :filter_text, parent: w)
      AND (u.sort_queued, u.sort_queue, u.sort_undated,
           u.sort_oldest, u.root_position, u.watched_folder_path,
           u.path_text, u.entry)
          >= (:order_key_0, :order_key_1, :order_key_2,
              :order_key_3, :order_key_4, :order_key_5,
              :order_key_6, :order_key_7)
      AND (w.sort_member, w.sort_undated, w.sort_oldest, w.path_text,
           w.key, w.entry)
          >= (
             CASE WHEN (u.sort_queued, u.sort_queue, u.sort_undated,
                 u.sort_oldest, u.root_position, u.watched_folder_path,
                 u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5,
                    :order_key_6, :order_key_7)
             THEN :order_key_8 ELSE -1 END,
             CASE WHEN (u.sort_queued, u.sort_queue, u.sort_undated,
                 u.sort_oldest, u.root_position, u.watched_folder_path,
                 u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5,
                    :order_key_6, :order_key_7)
             THEN :order_key_9 ELSE 0 END,
             CASE WHEN (u.sort_queued, u.sort_queue, u.sort_undated,
                 u.sort_oldest, u.root_position, u.watched_folder_path,
                 u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5,
                    :order_key_6, :order_key_7)
             THEN :order_key_10 ELSE 0 END,
             CASE WHEN (u.sort_queued, u.sort_queue, u.sort_undated,
                 u.sort_oldest, u.root_position, u.watched_folder_path,
                 u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5,
                    :order_key_6, :order_key_7)
             THEN :order_key_11 ELSE 0 END,
             CASE WHEN (u.sort_queued, u.sort_queue, u.sort_undated,
                 u.sort_oldest, u.root_position, u.watched_folder_path,
                 u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5,
                    :order_key_6, :order_key_7)
             THEN :order_key_12 ELSE 0 END,
             CASE WHEN (u.sort_queued, u.sort_queue, u.sort_undated,
                 u.sort_oldest, u.root_position, u.watched_folder_path,
                 u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5,
                    :order_key_6, :order_key_7)
             THEN :order_key_13 ELSE 0 END)
    ORDER BY u.sort_queued, u.sort_queue, u.sort_undated,
             u.sort_oldest, u.root_position, u.watched_folder_path,
             u.path_text, u.entry, w.sort_member,
             w.sort_undated, w.sort_oldest, w.path_text,
             w.key, w.entry
    LIMIT :limit
"#
);

// The window of the list by folder, A to Z.
sqlive_compose::read!(
    writer PathAscendingWindow(
        tab: String,
        filter: Option<String>,
        filter_text: Option<String>,
        folds_only: bool,
        folded: String,
        limit: i64,
        order_key_0: bae_store::list_windows::SortValue,
        order_key_1: bae_store::list_windows::SortValue,
        order_key_2: bae_store::list_windows::SortValue,
        order_key_3: bae_store::list_windows::SortValue,
        order_key_4: bae_store::list_windows::SortValue,
        order_key_5: bae_store::list_windows::SortValue,
        order_key_6: bae_store::list_windows::SortValue,
        order_key_7: bae_store::list_windows::SortValue,
        order_key_8: bae_store::list_windows::SortValue,
        order_key_9: bae_store::list_windows::SortValue
    ) {
        release_key: Option<crate::CandidateKey>, has_summary: Option<bool>,
        import_failed: Option<bool>, actionable: Option<bool>, ends_group: Option<bool>
    },
    r#"
    SELECT u.sort_queued AS order_key_0,
           u.sort_queue AS order_key_1,
           u.root_position AS order_key_2,
           u.watched_folder_path AS order_key_3,
           u.path_text AS order_key_4,
           u.entry AS order_key_5,
           w.sort_member AS order_key_6,
           w.path_text AS order_key_7,
           w.key AS order_key_8,
           w.entry AS order_key_9,
           w.item,
           p.watched_folder_path AS release_root, p.key AS release_key, p.has_summary,
           p.shown_title, p.shown_credits, p.tab, p.import_failed, p.actionable,
           p.pending_state, p.needs_you_reason, p.reason_pressings, p.error_detail,
           @crate::tables::import_list::row_fragments::ListedActions(parent: p) AS actions,
           @crate::tables::import_list::row_fragments::ListedRow(parent: p) AS listed,
           @crate::tables::import_list::row_fragments::ListedHeader(
               filter: :filter, filter_text: :filter_text, folds_only: :folds_only,
               folded: :folded, parent: w) AS header,
           CASE WHEN w.item <> 'group' AND w.group_folder IS NOT NULL THEN NOT EXISTS (
               SELECT 1 FROM import_list_placement AS m
               WHERE m.unit_entry = w.unit_entry
                 AND (m.sort_member, m.path_text, m.key, m.entry)
                     > (w.sort_member, w.path_text, w.key, w.entry)
                 AND @crate::tables::import_list::row_fragments::Kept(
                     filter: :filter, filter_text: :filter_text, parent: m)) END AS ends_group
    FROM import_list_placement AS u
    CROSS JOIN import_list_placement AS w
        ON w.unit_entry = u.entry
       AND w.sort_member <= CASE WHEN u.item = 'group' AND @crate::tables::import_list::row_fragments::Folded(
              folds_only: :folds_only, folded: :folded, parent: u) THEN 0 ELSE 1 END
    LEFT JOIN import_list_placement AS p ON p.entry = w.entry AND w.item <> 'group'
    WHERE u.tab = :tab AND u.unit_entry = u.entry
      AND @crate::tables::import_list::row_fragments::Kept(
          filter: :filter, filter_text: :filter_text, parent: w)
      AND (u.sort_queued, u.sort_queue, u.root_position,
           u.watched_folder_path, u.path_text, u.entry)
          >= (:order_key_0, :order_key_1, :order_key_2,
              :order_key_3, :order_key_4, :order_key_5)
      AND (w.sort_member, w.path_text, w.key, w.entry)
          >= (
             CASE WHEN (u.sort_queued, u.sort_queue, u.root_position,
                 u.watched_folder_path, u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5)
             THEN :order_key_6 ELSE -1 END,
             CASE WHEN (u.sort_queued, u.sort_queue, u.root_position,
                 u.watched_folder_path, u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5)
             THEN :order_key_7 ELSE 0 END,
             CASE WHEN (u.sort_queued, u.sort_queue, u.root_position,
                 u.watched_folder_path, u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5)
             THEN :order_key_8 ELSE 0 END,
             CASE WHEN (u.sort_queued, u.sort_queue, u.root_position,
                 u.watched_folder_path, u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5)
             THEN :order_key_9 ELSE 0 END)
    ORDER BY u.sort_queued, u.sort_queue, u.root_position,
             u.watched_folder_path, u.path_text, u.entry,
             w.sort_member, w.path_text, w.key,
             w.entry
    LIMIT :limit
"#
);

// The window of the list by folder, Z to A.
sqlive_compose::read!(
    writer PathDescendingWindow(
        tab: String,
        filter: Option<String>,
        filter_text: Option<String>,
        folds_only: bool,
        folded: String,
        limit: i64,
        order_key_0: bae_store::list_windows::SortValue,
        order_key_1: bae_store::list_windows::SortValue,
        order_key_2: bae_store::list_windows::SortValue,
        order_key_3: bae_store::list_windows::SortValue,
        order_key_4: bae_store::list_windows::SortValue,
        order_key_5: bae_store::list_windows::SortValue,
        order_key_6: bae_store::list_windows::SortValue,
        order_key_7: bae_store::list_windows::SortValue,
        order_key_8: bae_store::list_windows::SortValue,
        order_key_9: bae_store::list_windows::SortValue
    ) {
        release_key: Option<crate::CandidateKey>, has_summary: Option<bool>,
        import_failed: Option<bool>, actionable: Option<bool>, ends_group: Option<bool>
    },
    r#"
    SELECT u.sort_queued_back AS order_key_0,
           u.sort_queue_back AS order_key_1,
           u.root_position AS order_key_2,
           u.watched_folder_path AS order_key_3,
           u.path_text AS order_key_4,
           u.entry AS order_key_5,
           w.sort_header AS order_key_6,
           w.path_text AS order_key_7,
           w.key AS order_key_8,
           w.entry AS order_key_9,
           w.item,
           p.watched_folder_path AS release_root, p.key AS release_key, p.has_summary,
           p.shown_title, p.shown_credits, p.tab, p.import_failed, p.actionable,
           p.pending_state, p.needs_you_reason, p.reason_pressings, p.error_detail,
           @crate::tables::import_list::row_fragments::ListedActions(parent: p) AS actions,
           @crate::tables::import_list::row_fragments::ListedRow(parent: p) AS listed,
           @crate::tables::import_list::row_fragments::ListedHeader(
               filter: :filter, filter_text: :filter_text, folds_only: :folds_only,
               folded: :folded, parent: w) AS header,
           CASE WHEN w.item <> 'group' AND w.group_folder IS NOT NULL THEN NOT EXISTS (
               SELECT 1 FROM import_list_placement AS m
               WHERE m.unit_entry = w.unit_entry
                 AND (m.sort_header, m.path_text, m.key, m.entry)
                     < (w.sort_header, w.path_text, w.key, w.entry)
                 AND @crate::tables::import_list::row_fragments::Kept(
                     filter: :filter, filter_text: :filter_text, parent: m)) END AS ends_group
    FROM import_list_placement AS u
    CROSS JOIN import_list_placement AS w
        ON w.unit_entry = u.entry
       AND w.sort_header >= CASE WHEN u.item = 'group' AND @crate::tables::import_list::row_fragments::Folded(
              folds_only: :folds_only, folded: :folded, parent: u) THEN 1 ELSE 0 END
    LEFT JOIN import_list_placement AS p ON p.entry = w.entry AND w.item <> 'group'
    WHERE u.tab = :tab AND u.unit_entry = u.entry
      AND @crate::tables::import_list::row_fragments::Kept(
          filter: :filter, filter_text: :filter_text, parent: w)
      AND (u.sort_queued_back, u.sort_queue_back, u.root_position,
           u.watched_folder_path, u.path_text, u.entry)
          <= (:order_key_0, :order_key_1, :order_key_2,
              :order_key_3, :order_key_4, :order_key_5)
      AND (w.sort_header, w.path_text, w.key, w.entry)
          <= (
             CASE WHEN (u.sort_queued_back, u.sort_queue_back, u.root_position,
                 u.watched_folder_path, u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5)
             THEN :order_key_6 ELSE 2 END,
             CASE WHEN (u.sort_queued_back, u.sort_queue_back, u.root_position,
                 u.watched_folder_path, u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5)
             THEN :order_key_7 ELSE 0 END,
             CASE WHEN (u.sort_queued_back, u.sort_queue_back, u.root_position,
                 u.watched_folder_path, u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5)
             THEN :order_key_8 ELSE 0 END,
             CASE WHEN (u.sort_queued_back, u.sort_queue_back, u.root_position,
                 u.watched_folder_path, u.path_text, u.entry)
                 = (:order_key_0, :order_key_1, :order_key_2,
                    :order_key_3, :order_key_4, :order_key_5)
             THEN :order_key_9 ELSE 0 END)
    ORDER BY u.sort_queued_back DESC, u.sort_queue_back DESC, u.root_position DESC,
             u.watched_folder_path DESC, u.path_text DESC, u.entry DESC,
             w.sort_header DESC, w.path_text DESC, w.key DESC,
             w.entry DESC
    LIMIT :limit
"#
);
