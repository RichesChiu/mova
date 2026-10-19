-- The modification time of each local media file, as the scan inventory
-- observed it. Together with the size it identifies a file across a rename or
-- move, so its catalog row and the item's playback state follow the file.
-- Rows from earlier versions start as null and are filled in by the next scan.
alter table media_files
    add column file_modified_at_ms bigint;
