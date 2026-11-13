-- Daily digest: once a day per project, a short summary of the room, written
-- by Claude and posted to the room and its Telegram group. "Day" follows the
-- database time zone (alter database ... set timezone), not UTC.

alter table projects add column digest boolean not null default true;

create table daily_digests (
  project_id  uuid not null references projects (id) on delete cascade,
  day         date not null,
  -- sent | skipped (too quiet) | failed (retried a few times)
  status      text not null check (status in ('sent', 'skipped', 'failed')),
  attempts    int not null default 1,
  message_id  bigint references messages (id) on delete set null,
  error       text,
  updated_at  timestamptz not null default now(),
  primary key (project_id, day)
);
