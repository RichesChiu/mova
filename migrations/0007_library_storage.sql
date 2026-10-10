-- Library storage availability.
--
-- After each successful scan Mova records the mounts a library lives on: the
-- mount that holds the library root and every mount below it. Before missing
-- media is removed, the current mount table is compared with this record, so
-- storage that is not connected is never mistaken for deleted media. Existing
-- libraries need no rescan; each gains its record on its next successful scan.
create table if not exists library_storage_mounts (
    library_id bigint not null references libraries(id) on delete cascade,
    mount_point text not null,
    fs_type text not null,
    source text not null,
    recorded_at timestamptz not null default now(),
    primary key (library_id, mount_point),
    check (left(mount_point, 1) = '/'),
    check (btrim(fs_type) <> '')
);

alter table libraries
    add column storage_status varchar(16) not null default 'available',
    add column storage_issue jsonb,
    add column storage_unavailable_since timestamptz;

alter table libraries
    add constraint chk_libraries_storage_status
        check (
            (
                storage_status = 'available'
                and storage_issue is null
                and storage_unavailable_since is null
            )
            or
            (
                storage_status = 'unavailable'
                and jsonb_typeof(storage_issue) = 'object'
                and storage_unavailable_since is not null
            )
        );
