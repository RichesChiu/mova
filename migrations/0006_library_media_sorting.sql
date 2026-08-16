-- Keep the high-frequency library title sort index-aligned when an NFO
-- supplies sorttitle/sortname. Library detail and bounded home previews share
-- the same effective-title contract, so the superseded index is removed.
drop index if exists idx_media_items_library_display_title;

create index if not exists idx_media_items_library_effective_sort_title
    on media_items (
        library_id,
        lower(
            coalesce(
                nullif(sort_title, ''),
                nullif(title, ''),
                source_title
            )
        ),
        id
    )
    where media_type in ('movie', 'series');
