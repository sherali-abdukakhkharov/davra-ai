-- Very short messages and CI status notices sat close to every query and
-- crowded out real hits. They keep lexical search; they lose their vectors
-- (the embedding worker skips them from now on, see embed.rs EMBEDDABLE).
update messages set embedding = null
where embedding is not null
  and (length(btrim(body)) < 40 or (kind = 'system' and body like 'CI %'));
