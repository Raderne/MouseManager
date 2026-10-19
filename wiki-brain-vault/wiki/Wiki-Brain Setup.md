# Wiki-Brain Setup

This project has a **Wiki-Brain** knowledge base at `wiki-brain-vault/`. The setup was copied from `C:\dev\Algo\Assurflow\AgenticAI` on 2026-10-19.

- **Vault:** `C:\dev\projects\MouseManager\wiki-brain-vault`
- **CLAUDE.md integration:** a Wiki-Brain block in the repo-root `CLAUDE.md` tells Claude to read the wiki first, keep it updated and log each session. See [[CLAUDE.md Rules]].

## Layout

```
wiki-brain-vault/
├── raw/            # immutable sources (log dumps, descriptors, notes)
├── wiki/           # pages maintained by Claude (this folder)
│   └── index.md    # catalogue
└── log.md          # append-only session journal
```

The folder opens as an Obsidian vault. `[[Page]]` links are Obsidian wiki links.

## Tooling

The `/wiki-brain` skill and `graphify` are **not installed** on this machine. Until they are, read `wiki/index.md` and follow the links directly. To ingest a source, summarize it into a page by hand, as described in `raw/README.md`.

Related: [[N230 Control]].
