-- v0.2: semantic search. bge-m3 produces 1024-dimensional vectors.
create extension if not exists vector;

alter table messages add column embedding vector(1024);
alter table messages add column embed_attempts smallint not null default 0;

create index messages_embedding_idx on messages using hnsw (embedding vector_cosine_ops);
create index messages_unembedded_idx on messages (id) where embedding is null;
