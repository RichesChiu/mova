alter table media_files
    add column source_kind varchar(16) not null default 'local_file',
    add column stream_reference_hash varchar(64);

alter table media_files
    add constraint chk_media_files_source_kind
        check (source_kind in ('local_file', 'strm')),
    add constraint chk_media_files_stream_reference
        check (
            (source_kind = 'local_file' and stream_reference_hash is null)
            or
            (
                source_kind = 'strm'
                and stream_reference_hash is not null
                and length(stream_reference_hash) = 64
                and stream_reference_hash ~ '^[0-9a-f]{64}$'
            )
        );

-- Intro detection consumes local media bytes only. Keep the existing trigger
-- but make it ignore STRM-only changes, while still invalidating the local
-- side when a row changes source category.
create or replace function mova_invalidate_intro_after_media_file_change()
returns trigger
language plpgsql
as $$
declare
    previous_season_id bigint;
    current_season_id bigint;
begin
    if tg_op = 'INSERT' and new.source_kind <> 'local_file' then
        return new;
    end if;

    if tg_op = 'DELETE' and old.source_kind <> 'local_file' then
        return old;
    end if;

    if tg_op = 'UPDATE'
       and old.source_kind <> 'local_file'
       and new.source_kind <> 'local_file' then
        return new;
    end if;

    if tg_op = 'UPDATE'
       and new.media_item_id is not distinct from old.media_item_id
       and new.file_path is not distinct from old.file_path
       and new.file_size is not distinct from old.file_size
       and new.duration_seconds is not distinct from old.duration_seconds
       and new.scan_hash is not distinct from old.scan_hash
       and new.source_kind is not distinct from old.source_kind
       and new.stream_reference_hash is not distinct from old.stream_reference_hash then
        return new;
    end if;

    if tg_op in ('DELETE', 'UPDATE') and old.source_kind = 'local_file' then
        select episode.season_id
        into previous_season_id
        from episodes episode
        where episode.media_item_id = old.media_item_id;
    end if;

    if tg_op in ('INSERT', 'UPDATE') and new.source_kind = 'local_file' then
        select episode.season_id
        into current_season_id
        from episodes episode
        where episode.media_item_id = new.media_item_id;
    end if;

    perform mova_invalidate_season_intro_analysis(previous_season_id);
    if current_season_id is distinct from previous_season_id then
        perform mova_invalidate_season_intro_analysis(current_season_id);
    end if;

    if tg_op = 'DELETE' then
        return old;
    end if;
    return new;
end;
$$;

-- Delete invalidation must run while the episode row is still visible. The
-- original deferred trigger could no longer resolve the affected season after
-- the last carrier and its orphan episode were removed in the same transaction.
drop trigger media_files_intro_analysis_delete_invalidation on media_files;
create trigger media_files_intro_analysis_delete_invalidation
after delete on media_files
for each row execute function mova_invalidate_intro_after_media_file_change();

-- Episode hierarchy changes matter only when that episode has local bytes.
-- A newly inserted local episode is also covered when its media_file row is
-- inserted later in the transaction; a STRM-only episode never becomes an
-- intro input and therefore must not clear otherwise valid season markers.
create or replace function mova_invalidate_intro_after_episode_change()
returns trigger
language plpgsql
as $$
declare
    previous_season_id bigint;
    current_season_id bigint;
begin
    if tg_op = 'UPDATE'
       and new.season_id is not distinct from old.season_id
       and new.media_item_id is not distinct from old.media_item_id
       and new.episode_number is not distinct from old.episode_number then
        return new;
    end if;

    if tg_op in ('DELETE', 'UPDATE')
       and exists (
            select 1
            from media_files media_file
            where media_file.media_item_id = old.media_item_id
              and media_file.source_kind = 'local_file'
       ) then
        previous_season_id := old.season_id;
    end if;

    if tg_op in ('INSERT', 'UPDATE')
       and exists (
            select 1
            from media_files media_file
            where media_file.media_item_id = new.media_item_id
              and media_file.source_kind = 'local_file'
       ) then
        current_season_id := new.season_id;
    end if;

    perform mova_invalidate_season_intro_analysis(previous_season_id);
    if current_season_id is distinct from previous_season_id then
        perform mova_invalidate_season_intro_analysis(current_season_id);
    end if;

    if tg_op = 'DELETE' then
        return old;
    end if;
    return new;
end;
$$;

drop trigger episodes_intro_analysis_delete_invalidation on episodes;
create trigger episodes_intro_analysis_delete_invalidation
after delete on episodes
for each row execute function mova_invalidate_intro_after_episode_change();
