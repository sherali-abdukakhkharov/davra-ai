-- Decision log: what the team decided, by whom, why, and instead of what.
-- Agents record decisions (decision_record); the repository keeps them as
-- files in docs/memory/decisions/, and the brief shows the latest ones.

create table decisions (
  id              bigserial primary key,
  org_id          uuid not null references orgs (id) on delete cascade,
  project_id      uuid not null references projects (id) on delete cascade,
  author_id       uuid references members (id) on delete set null,
  decided_by      text not null,
  ticket          text,
  title           text not null,
  decision        text not null,
  why             text not null,
  instead         text,
  status          text not null default 'accepted' check (status in ('accepted', 'superseded')),
  supersedes      bigint references decisions (id) on delete set null,
  superseded_by   bigint references decisions (id) on delete set null,
  source_message  bigint references messages (id) on delete set null,
  message_id      bigint references messages (id) on delete set null,
  file_path       text,
  created_at      timestamptz not null default now()
);

create index decisions_project on decisions (project_id, id desc);
create index decisions_ticket on decisions (project_id, ticket) where ticket is not null;

-- Decision announcements are records, like handoff cards (the loop guard skips both).
alter table messages drop constraint messages_kind_check;
alter table messages add constraint messages_kind_check
  check (kind in ('chat', 'handoff', 'system', 'decision'));

do $rls$
begin
  if exists (select 1 from pg_roles where rolname = 'davra_app') then
    grant select, insert, update on decisions to davra_app;
    grant usage, select on sequence decisions_id_seq to davra_app;
    alter table decisions enable row level security;
    create policy tenant on decisions to davra_app
      using (org_id = davra_org()) with check (org_id = davra_org());
  end if;
end
$rls$;
