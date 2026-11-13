-- Row-level security: tenant isolation enforced by Postgres itself.
--
-- The hub connects as the owner role, which bypasses RLS. Requests that act
-- for one org run in a transaction that switches to `davra_app`
-- (`set local role davra_app`) and sets `davra.org` to that org's id. For
-- that role every tenant table shows only that org's rows, so a query that
-- forgets its org filter still cannot read or write another org's data.
-- With `davra.org` unset, `davra_app` sees nothing (fail closed).
--
-- Creating a role needs CREATEROLE. Where the hub's database user lacks it,
-- this migration changes nothing and the hub logs that RLS is off.

create function davra_org() returns uuid language sql stable as $$
  select nullif(current_setting('davra.org', true), '')::uuid
$$;

do $rls$
begin
  if not exists (select 1 from pg_roles where rolname = 'davra_app') then
    begin
      create role davra_app nologin;
    exception when insufficient_privilege then
      raise warning 'davra: cannot create role davra_app (needs CREATEROLE); row-level security stays off';
      return;
    end;
  end if;
  -- The hub's own user must be able to `set role davra_app`.
  begin
    execute format('grant davra_app to %I', current_user);
  exception when others then
    raise warning 'davra: could not grant davra_app to %: %', current_user, sqlerrm;
  end;

  grant usage on schema public to davra_app;
  grant usage, select on all sequences in schema public to davra_app;
  grant execute on function davra_org() to davra_app;

  -- Tenant tables with an org_id column.
  grant select, insert, update, delete on projects, members, messages, handoffs, questions to davra_app;
  grant select, insert on audit_log to davra_app;
  grant select on orgs to davra_app;
  -- Tenant tables keyed through a project or a member.
  grant select, insert, update, delete on grants, tokens, cursors, invites to davra_app;
  -- Accounts are global; an org-scoped request may read names, never sessions.
  grant select on users to davra_app;

  alter table orgs enable row level security;
  create policy tenant on orgs to davra_app using (id = davra_org());

  alter table projects enable row level security;
  create policy tenant on projects to davra_app
    using (org_id = davra_org()) with check (org_id = davra_org());
  alter table members enable row level security;
  create policy tenant on members to davra_app
    using (org_id = davra_org()) with check (org_id = davra_org());
  alter table messages enable row level security;
  create policy tenant on messages to davra_app
    using (org_id = davra_org()) with check (org_id = davra_org());
  alter table handoffs enable row level security;
  create policy tenant on handoffs to davra_app
    using (org_id = davra_org()) with check (org_id = davra_org());
  alter table questions enable row level security;
  create policy tenant on questions to davra_app
    using (org_id = davra_org()) with check (org_id = davra_org());
  alter table audit_log enable row level security;
  create policy tenant on audit_log to davra_app
    using (org_id = davra_org()) with check (org_id = davra_org());

  alter table grants enable row level security;
  create policy tenant on grants to davra_app
    using (exists (select 1 from projects p where p.id = project_id and p.org_id = davra_org()))
    with check (exists (select 1 from projects p where p.id = project_id and p.org_id = davra_org()));
  alter table invites enable row level security;
  create policy tenant on invites to davra_app
    using (exists (select 1 from projects p where p.id = project_id and p.org_id = davra_org()))
    with check (exists (select 1 from projects p where p.id = project_id and p.org_id = davra_org()));
  alter table tokens enable row level security;
  create policy tenant on tokens to davra_app
    using (exists (select 1 from members m where m.id = member_id and m.org_id = davra_org()))
    with check (exists (select 1 from members m where m.id = member_id and m.org_id = davra_org()));
  alter table cursors enable row level security;
  create policy tenant on cursors to davra_app
    using (exists (select 1 from members m where m.id = member_id and m.org_id = davra_org()))
    with check (exists (select 1 from members m where m.id = member_id and m.org_id = davra_org()));
end
$rls$;
