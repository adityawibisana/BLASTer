# Example data

Test batches for the app. Import a zip, a folder or single files from the Start screen, or drop them on the window.

| Path | What | Reads |
|------|------|-------|
| `COI_20_sequences.zip` | Mosquito COI, single reads | 20 `.ab1` |
| `sangeranalyseR/` | Forward/reverse pairs from BOLD | 26 `.ab1` (13 pairs) |
| `fasta/sangeranalyseR_reads.fa` | FASTA reads, for FASTA import | 8 records |

## `COI_20_sequences.zip`

20 COI barcode reads from mosquitoes. Every file is a single read; the `-1`/`-2`/`-3` suffixes are isolate numbers, not directions. This is the batch the Phase 1 checks and screenshots use (see the main README).

## `sangeranalyseR/`

Real Sanger traces from [BOLD Systems](https://www.boldsystems.org/), downloaded by the authors of the R package [sangeranalyseR](https://github.com/roblanf/sangeranalyseR) in January 2016 and shipped as its test data (`inst/extdata`). The package's README there explains the renaming: BOLD's `ACHLO006-09[LCO1490_t1,HCO2198_t1]_1_F.ab1` became `ACHLO006-09_1_F.ab1`, then a species prefix was added (`Achl_ACHLO006-09_1_F.ab1`). The part after the prefix is the BOLD process ID and can be looked up on BOLD.

sangeranalyseR is released under GPL-2; the traces are public BOLD records.

| Folder | Species | BOLD process IDs | Primers |
|--------|---------|------------------|---------|
| `Allolobophora_chlorotica/` | *Allolobophora chlorotica* (green earthworm) | ACHLO006-09, ACHLO007-09, ACHLO040-09, ACHLO041-09, RBNII384-13, RBNII395-13, RBNII396-13, RBNII397-13 | LCO1490 / HCO2198, M13-tailed |
| `Drosophila_melanogaster/` | *Drosophila melanogaster* (fruit fly) | BBDCN941-10, BBDEE689-10, PHDIP946-11, TDWGB557-10, TDWGB669-10 | LCO1490 / HCO2198, or the C_LepFolF / C_LepFolR cocktail (LepF1 + LCO1490 / LepR1 + HCO2198); see `TRACE_FILE_INFO.txt` |

Each specimen has a forward (`_1_F`) and a reverse (`_2_R`) read, so this batch exercises pairing and assembly on real traces.

### What the app gives (checked 24 Nov 2026)

Import the folder, press *Pair all*, search NCBI (`core_nt`, megablast) and compile:

| Specimen | Consensus | Closest species | % identity | Flags | Notes |
|----------|-----------|-----------------|-----------:|-------|-------|
| ACHLO006-09 | 629 bp | *Allolobophora chlorotica* | 100.00 | | |
| ACHLO007-09 | 658 bp | *Allolobophora chlorotica* | 100.00 | | Full Folmer barcode |
| ACHLO040-09 | 658 bp | *Allolobophora chlorotica* | 100.00 | | |
| ACHLO041-09 | 658 bp | *Allolobophora chlorotica* | 100.00 | | |
| RBNII384-13 | 653 bp | *Allolobophora chlorotica* | 100.00 | F/R conflict | One position where both reads are high quality but disagree: worth a look on the chromatogram |
| RBNII395-13 | 471 bp | *Allolobophora chlorotica* | 99.79 | Length | The reverse read is poor (57% HQ) and adds nothing past the forward read |
| RBNII396-13 | 591 bp | *Allolobophora chlorotica* | 100.00 | Length | ~30 bases at each barcode end are below Q40 in both reads |
| RBNII397-13 | 560 bp | *Allolobophora chlorotica* | 100.00 | Length | As above |
| BBDCN941-10 | 608 bp | *Drosophila melanogaster* | 99.84 | Mixed species | A *D. hydei* record matches equally: almost certainly a mislabelled GenBank entry |
| BBDEE689-10 | 632 bp | *Drosophila melanogaster* | 99.84 | | |
| PHDIP946-11 | 654 bp | *Drosophila melanogaster* | 100.00 | | |
| TDWGB557-10 | 649 bp | *Drosophila melanogaster* | 100.00 | | 18 F/R disagreements, all resolved by quality |
| TDWGB669-10 | 603 bp | *Drosophila melanogaster* | 100.00 | Mixed species | Same *D. hydei* record |

- Every read's direction, found from its sequence, matches its file name (13 F, 13 R).
- *A. chlorotica* is a complex of cryptic lineages. The runner-up for these samples is an *A. chlorotica complex sp. L2* or *L4* record, which shows the lineage; such placeholder names are not counted as a different species.
- The *Length* flag means the consensus is shorter than the expected 600–670 bp. The identification is still sound (BOLD counts 500 bp and up as barcode-compliant); it says the barcode ends were not read at high quality.

To repeat the check from the command line (`--blast` caches NCBI's answers in the given file):

```powershell
cd src-tauri
cargo run --example evaluate -- ../samples/sangeranalyseR --blast blast-cache.json `
  --expect "Achl=Allolobophora chlorotica" --expect "Dmel=Drosophila melanogaster"
```

## `fasta/sangeranalyseR_reads.fa`

The FASTA reads from the same package (`inst/extdata/fasta/SangerAlignment/Sanger_all_reads.fa`): the forward and reverse reads of the four ACHLO earthworm specimens as plain sequence, without quality values. It is kept apart from the `.ab1` folder because it repeats those reads.
