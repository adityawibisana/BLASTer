# Changelog

## v0.2.0 - 2026-11-26

- Match the Tauri Rust crates to the npm packages (2.12, dialog 2.8, opener 2.7) so tauri build passes (bb48631)
- Fix CI: add the new failures field to the BlastGroups built in tests and the evaluate example (521a741)
- Retry failed searches automatically, 3 tries in all; give up after 45 min by default (8eea48d)
- Check NCBI search status every 30 seconds (40f9fe9)
- Set the give-up time for a waiting NCBI search in minutes (c23896c)
- Speed up slow NCBI BLAST: fewer hits, NCBI website, BLAST+ on this computer (1b2db1b)
- Add Processes tab: live NCBI search status, send again, close confirmation (0cfcd46)
- Give up on a BLAST search NCBI leaves waiting for 2 hours (bf61bdd)
- Add GitHub Sponsors funding link (3cbc220)
- Add Help Us tab with the open questions for the lab (b6e6bd2)
- Verify built-in primer sequences (To verify #3) (b31931c)
- Verify Geneious default trim limit of 0.05 (To verify #2) (7b3de8f)
- Verify NCBI RID expiry (To verify #4) (f420373)
- Add nightly release workflow (fb29e7d)
- Update README and screenshots (72da16f)
- Import folders and mixed files, pair BOLD names, prefer named species (4d62a38)
- Add example data in samples/, with 13 BOLD forward/reverse pairs (3d61281)
- Add screenshots and Highlights section to README (e4e1546)
- Import from FASTA (ea0a908)
- Add VS Code launch configs for running the app (e4d9c36)
- Phase 2: forward/reverse pairs and PDF report; BOLD dropped (ac6c134)
- COI Barcode BLAST: Phase 1 desktop app (f6b8bae)

