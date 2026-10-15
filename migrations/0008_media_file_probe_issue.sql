-- Why ffprobe could not analyze a media file.
--
-- A file ffprobe cannot analyze is still catalogued and scraped; this column
-- carries the category shown to users so they know the file itself may not
-- play. Missing individual fields (resolution, duration, codec) stay null and
-- are not an issue. Existing rows need no rescan: the column fills in as files
-- are probed.
alter table media_files
    add column probe_issue varchar(32);

alter table media_files
    add constraint chk_media_files_probe_issue
        check (
            probe_issue is null
            or probe_issue in ('unreadable', 'unrecognized', 'no_video_stream', 'timeout')
        );
