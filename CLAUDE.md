# JARVIS — punto di ingresso per Claude Code

La documentazione approvata si trova in `docs/`. Le istruzioni operative complete sono in [docs/CLAUDE.md](docs/CLAUDE.md); il checkpoint di ripresa è in [docs/ROADMAP.md](docs/ROADMAP.md).

@docs/CLAUDE.md

Note di percorso: i riferimenti nei documenti (README.md, ROADMAP.md, ...) sono relativi a `docs/`. Contratti in `contracts/`, Core Rust in `core/`, verifiche trasversali in `tests/`. Comandi di verifica correnti (gli stessi della CI `.github/workflows/contracts.yml`):

```bash
python3 -m unittest discover -s tests/contracts -v
```

```bash
cd core && cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked
```

A fine sessione aggiorna sempre il checkpoint in `docs/ROADMAP.md` e le specifiche toccate in `docs/`.
