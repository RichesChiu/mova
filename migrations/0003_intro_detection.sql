create table if not exists season_intro_analyses (
    season_id bigint primary key references seasons(id) on delete cascade,
    algorithm_version integer not null,
    input_fingerprint varchar(64),
    outcome varchar(16) not null,
    intro_start_seconds integer,
    intro_end_seconds integer,
    confidence double precision,
    sampled_episode_count integer not null default 0,
    analyzed_episode_count integer not null default 0,
    failed_episode_count integer not null default 0,
    reason_code varchar(64),
    retry_after timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    check (algorithm_version > 0),
    check (outcome in ('matched', 'no_match', 'failed')),
    check (input_fingerprint is null or input_fingerprint ~ '^[0-9a-f]{64}$'),
    check (intro_start_seconds is null or intro_start_seconds >= 0),
    check (intro_end_seconds is null or intro_end_seconds >= 0),
    check (confidence is null or confidence between 0.0 and 1.0),
    check (sampled_episode_count >= 0),
    check (analyzed_episode_count >= 0),
    check (failed_episode_count >= 0),
    check (analyzed_episode_count + failed_episode_count <= sampled_episode_count),
    check (reason_code is null or reason_code ~ '^[a-z0-9][a-z0-9_]*$'),
    check (
        (
            outcome = 'matched'
            and input_fingerprint is not null
            and intro_start_seconds is not null
            and intro_end_seconds is not null
            and intro_end_seconds > intro_start_seconds
            and confidence is not null
            and reason_code is null
            and retry_after is null
        )
        or (
            outcome = 'no_match'
            and input_fingerprint is not null
            and intro_start_seconds is null
            and intro_end_seconds is null
            and confidence is null
            and reason_code is not null
            and retry_after is null
        )
        or (
            outcome = 'failed'
            and intro_start_seconds is null
            and intro_end_seconds is null
            and confidence is null
            and reason_code is not null
            and retry_after is not null
        )
    )
);

create index if not exists idx_season_intro_analyses_retry
    on season_intro_analyses(retry_after, season_id)
    where outcome = 'failed';

create unique index if not exists idx_background_jobs_unique_active_intro_detection
    on background_jobs (job_type, ((payload ->> 'season_id')))
    where job_type = 'media.intro.detect'
      and status in ('pending', 'running', 'cancel_requested');

create or replace function mova_invalidate_season_intro_analysis(target_season_id bigint)
returns void
language plpgsql
as $$
begin
    if target_season_id is null then
        return;
    end if;

    delete from season_intro_analyses
    where season_id = target_season_id;

    update seasons
    set intro_start_seconds = null,
        intro_end_seconds = null,
        updated_at = now()
    where id = target_season_id
      and (intro_start_seconds is not null or intro_end_seconds is not null);
end;
$$;

create or replace function mova_invalidate_intro_after_media_file_change()
returns trigger
language plpgsql
as $$
declare
    previous_season_id bigint;
    current_season_id bigint;
begin
    if tg_op = 'UPDATE'
       and new.media_item_id is not distinct from old.media_item_id
       and new.file_path is not distinct from old.file_path
       and new.file_size is not distinct from old.file_size
       and new.duration_seconds is not distinct from old.duration_seconds
       and new.scan_hash is not distinct from old.scan_hash then
        return new;
    end if;

    if tg_op = 'DELETE' then
        select episode.season_id
        into previous_season_id
        from episodes episode
        where episode.media_item_id = old.media_item_id;
    elsif tg_op = 'UPDATE' then
        select episode.season_id
        into previous_season_id
        from episodes episode
        where episode.media_item_id = old.media_item_id;
    end if;

    if tg_op = 'INSERT' then
        select episode.season_id
        into current_season_id
        from episodes episode
        where episode.media_item_id = new.media_item_id;
    elsif tg_op = 'UPDATE' then
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

create trigger media_files_intro_analysis_invalidation
after insert or update on media_files
for each row execute function mova_invalidate_intro_after_media_file_change();

create constraint trigger media_files_intro_analysis_delete_invalidation
after delete on media_files
deferrable initially deferred
for each row execute function mova_invalidate_intro_after_media_file_change();

create or replace function mova_invalidate_intro_after_episode_change()
returns trigger
language plpgsql
as $$
begin
    if tg_op = 'UPDATE'
       and new.season_id is not distinct from old.season_id
       and new.media_item_id is not distinct from old.media_item_id
       and new.episode_number is not distinct from old.episode_number then
        return new;
    end if;

    if tg_op = 'DELETE' then
        perform mova_invalidate_season_intro_analysis(old.season_id);
    elsif tg_op = 'INSERT' then
        perform mova_invalidate_season_intro_analysis(new.season_id);
    elsif tg_op = 'UPDATE' then
        perform mova_invalidate_season_intro_analysis(old.season_id);
        if new.season_id is distinct from old.season_id then
            perform mova_invalidate_season_intro_analysis(new.season_id);
        end if;
    end if;

    if tg_op = 'DELETE' then
        return old;
    end if;
    return new;
end;
$$;

create trigger episodes_intro_analysis_invalidation
after insert or update on episodes
for each row execute function mova_invalidate_intro_after_episode_change();

create constraint trigger episodes_intro_analysis_delete_invalidation
after delete on episodes
deferrable initially deferred
for each row execute function mova_invalidate_intro_after_episode_change();
