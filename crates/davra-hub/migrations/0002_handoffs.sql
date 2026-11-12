-- v0.2: handoffs between agents (or roles), mirroring a repo's handoff files.

-- Roles an agent (or human) covers, e.g. {core, auth, db}. Handoffs addressed
-- to a role reach every member that covers it.
alter table members add column roles text[] not null default '{}';

create table handoffs (
  id          bigserial primary key,
  org_id      uuid not null references orgs (id) on delete cascade,
  project_id  uuid not null references projects (id) on delete cascade,
  ticket      text,
  from_label  text not null,
  to_labels   text[] not null,
  status      text not null default 'open' check (status in ('open', 'done')),
  body        text not null,
  done_note   text,
  source_path text,
  source_hash text,
  author_id   uuid references members (id) on delete set null,
  done_by     uuid references members (id) on delete set null,
  message_id  bigint references messages (id) on delete set null,
  created_at  timestamptz not null default now(),
  updated_at  timestamptz not null default now(),
  done_at     timestamptz
);

create index handoffs_open_idx on handoffs (project_id, status);
create index handoffs_to_idx on handoffs using gin (to_labels);
create unique index handoffs_source_idx on handoffs (project_id, source_path) where source_path is not null;
