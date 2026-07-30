drop index if exists idx_continue_watching_last_media_file_id;

alter table continue_watching
    drop column if exists last_media_file_id;
