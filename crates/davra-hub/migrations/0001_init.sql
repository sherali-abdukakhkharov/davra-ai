-- Davra v0.1 schema.
-- Every tenant-owned table carries org_id so row-level security can be
-- switched on in v0.3 without reshaping data.

create extension if not exists pg_trgm;

create table orgs (
  id         uuid primary key default gen_random_uuid(),
  slug       text not null unique,
  name       text,
  mode       text not null default 'standard' check (mode in ('standard', 'workplace')),
  created_at timestamptz not null default now()
);

create table projects (
  id          uuid primary key default gen_random_uuid(),
  org_id      uuid not null references orgs (id) on delete cascade,
  slug        text not null,
  name        text,
  tg_chat_id  bigint unique,
  created_at  timestamptz not null default now(),
  unique (org_id, slug)
);

create table members (
  id           uuid primary key default gen_random_uuid(),
  org_id       uuid not null references orgs (id) on delete cascade,
  kind         text not null check (kind in ('human', 'agent')),
  handle       text not null,
  display_name text,
  owner_id     uuid references members (id) on delete cascade,
  tg_user_id   bigint,
  created_at   timestamptz not null default now(),
  unique (org_id, handle),
  unique (org_id, tg_user_id),
  check (kind = 'agent' or owner_id is null)
);

create table grants (
  member_id  uuid not null references members (id) on delete cascade,
  project_id uuid not null references projects (id) on delete cascade,
  role       text not null check (role in ('owner', 'maintainer', 'member', 'viewer', 'agent')),
  primary key (member_id, project_id)
);

create table tokens (
  id           uuid primary key default gen_random_uuid(),
  member_id    uuid not null references members (id) on delete cascade,
  token_hash   bytea not null unique,
  name         text,
  created_at   timestamptz not null default now(),
  last_used_at timestamptz,
  revoked_at   timestamptz
);

create table messages (
  id            bigserial primary key,
  org_id        uuid not null references orgs (id) on delete cascade,
  project_id    uuid not null references projects (id) on delete cascade,
  author_id     uuid references members (id) on delete set null,
  author_name   text,
  kind          text not null default 'chat' check (kind in ('chat', 'handoff', 'system')),
  topic         text,
  body          text not null,
  reply_to      bigint references messages (id) on delete set null,
  mentions      text[] not null default '{}',
  source        text not null check (source in ('telegram', 'api', 'import')),
  tg_message_id bigint,
  tg_thread_id  bigint,
  created_at    timestamptz not null default now(),
  edited_at     timestamptz,
  tsv           tsvector generated always as (to_tsvector('simple', body)) stored
);

create index messages_project_id_idx on messages (project_id, id);
create index messages_tsv_idx on messages using gin (tsv);
create index messages_body_trgm_idx on messages using gin (body gin_trgm_ops);
create index messages_mentions_idx on messages using gin (mentions);
create unique index messages_tg_idx on messages (project_id, tg_message_id) where tg_message_id is not null;

-- Per member, per project: id of the last message the member acknowledged.
create table cursors (
  member_id  uuid not null references members (id) on delete cascade,
  project_id uuid not null references projects (id) on delete cascade,
  last_ack   bigint not null default 0,
  primary key (member_id, project_id)
);

-- Append-only. In v0.3 the hub role gets INSERT/SELECT only on this table.
create table audit_log (
  id       bigserial primary key,
  org_id   uuid references orgs (id) on delete cascade,
  actor_id uuid,
  action   text not null,
  target   text,
  detail   jsonb,
  at       timestamptz not null default now()
);

-- Telegram long-polling offset survives restarts.
create table kv (
  key   text primary key,
  value text not null
);
