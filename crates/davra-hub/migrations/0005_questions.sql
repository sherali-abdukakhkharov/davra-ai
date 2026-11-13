-- v0.2: an agent asks its owner to decide (ask_owner).
create table questions (
  id            bigserial primary key,
  org_id        uuid not null references orgs (id) on delete cascade,
  project_id    uuid not null references projects (id) on delete cascade,
  agent_id      uuid not null references members (id) on delete cascade,
  owner_id      uuid not null references members (id) on delete cascade,
  question      text not null,
  options       text[] not null default '{}',
  answer        text,
  message_id    bigint references messages (id) on delete set null,
  created_at    timestamptz not null default now(),
  answered_at   timestamptz
);

create index questions_open_idx on questions (owner_id) where answer is null;
