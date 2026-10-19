-- A scan may inspect only part of a library: the directories that held the
-- files of an item a user opened and found missing. Null means the whole
-- library; a scoped scan touches nothing outside these directories.
alter table scan_jobs
    add column scope_paths text[];
