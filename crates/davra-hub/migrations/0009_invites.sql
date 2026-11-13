-- v0.3: invite links. An owner or maintainer creates a single-use link for a
-- project and role; whoever opens it while signed in joins the project. The
-- link can also hand over an existing human member that has no web account
-- yet (one added from the CLI), so their history and handle stay theirs.

create table invites (
  id          uuid primary key default gen_random_uuid(),
  code_hash   bytea not null unique,
  project_id  uuid not null references projects (id) on delete cascade,
  role        text not null check (role in ('maintainer', 'member', 'viewer')),
  member_id   uuid references members (id) on delete cascade,
  created_by  uuid references members (id) on delete set null,
  created_at  timestamptz not null default now(),
  expires_at  timestamptz not null,
  used_at     timestamptz,
  used_by     uuid references users (id) on delete set null,
  revoked_at  timestamptz
);

create index invites_project_idx on invites (project_id) where used_at is null and revoked_at is null;
