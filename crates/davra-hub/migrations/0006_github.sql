-- v0.2: GitHub webhooks per project (push, pull requests, CI results).
-- The secret is needed in clear text to verify HMAC signatures; v0.3 moves it
-- under the hub's master key.
alter table projects add column github_repo text;
alter table projects add column github_secret text;
