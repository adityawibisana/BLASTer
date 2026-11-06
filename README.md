# COI Barcode BLAST

A Windows desktop app that takes a batch of Sanger sequencing trace files and prepares each one for identification:

- removes the PCR primers,
- trims each read to its high-quality region,
- puts every read in the forward direction,
- converts the result to FASTA.

It then runs the FASTA against NCBI BLAST and shows the hits in the same form as the NCBI BLAST web page: a results summary, a ranked list of similar sequences, and the pairwise alignment for each hit.

A **Compile results** button then decides automatically which species is closest for every sample and charts the species across the batch. Results can be exported to Excel.

A batch can hold up to **1000 samples**.

Phase 2 adds forward/reverse pair assembly, a PDF report, and an optional BOLD search (see [Phases](#phases)).

It automates what is done by hand today: open the traces in Geneious, trim them, copy each sequence into the NCBI BLAST form, and read the results page one sample at a time.

Main reference: NCBI's open-source tools and APIs at <https://github.com/ncbi>.

---

## Tech stack

A desktop app built with Tauri: a Rust backend and a React frontend, using the same setup as [r-shell](https://github.com/GOODBOY008/r-shell/blob/main/package.json).

| Layer    | Choice                                                                 |
|----------|------------------------------------------------------------------------|
| Platform | **Windows only** (Windows 10/11, x64). Uses the Microsoft Edge WebView2 runtime, which is built into Windows 11 and installed by the installer on Windows 10 |
| Shell    | Tauri 2, packaged as an NSIS `.exe` installer (and/or `.msi`)          |
| Backend  | Rust: `tokio` (async), `reqwest` (HTTP), `serde`/`serde_json`, `zip`, `anyhow`/`thiserror` |
| Export   | Rust: `rust_xlsxwriter` (Excel `.xlsx`), plus a Rust PDF generator (library to be chosen at implementation, e.g. `typst` or `printpdf`) |
| Frontend | React 19 + TypeScript, Vite, Tailwind CSS 4, Radix UI components, `lucide-react` icons |
| UI parts | `react-resizable-panels` (split views), `sonner` (notifications), `i18next` (English / Indonesian), `recharts` (species chart), `@tanstack/react-virtual` (tables with 1000+ rows stay smooth) |
| Tauri plugins | `dialog` (open zip, save export), `fs`, `opener` (GenBank links in browser) |
| Tooling  | pnpm, ESLint, Vitest (frontend), `cargo test` (backend)                |

The Rust backend does the heavy work: unpacking, ABIF parsing, trimming, primer removal, assembly, export, and all network calls to NCBI. The React frontend only displays results and sends commands through Tauri `invoke`.

## Input

A `.zip` archive of Sanger chromatogram files in ABIF format (`.ab1`), with **up to 1000 samples per batch**. Reads can be:

- **Single reads:** one `.ab1` per sample, sequenced in either direction.
- **Forward + reverse pairs:** two `.ab1` files per sample, assembled into one consensus sequence before BLAST.

Filenames do not need to say which read is forward or reverse. The app detects each read's direction from its sequence (see step 5).

Example: `COI_20_sequences.zip` holds 20 COI (cytochrome c oxidase subunit I) barcode reads:

```
VN220.ab1     VN222-2.ab1   VN234-2.ab1   VN236-1.ab1   VN239-2.ab1
VN241-2.ab1   VN242-2.ab1   VN248-2.ab1   VN251.ab1     VN24-1.ab1
VN282-1.ab1   VN290-3.ab1   VN31-1.ab1    VN32-1.ab1    VN42-2.ab1
VN60-1.ab1    14-2022.ab1   30-2022.ab1   49-2022.ab1   274-2022.ab1
```

## Pipeline

```
 .zip ─► unpack ─► read .ab1 ─► find primers ─► HQ trim ─► manual review ─► pair F/R ─► assemble ─► .fasta ─► BLAST ─► results ─► Excel / PDF
                                (outer limit)   (inner)    (drag limits)       │                              │
                                                                               └─ single reads skip assembly  └─ NCBI (+ BOLD, optional)
```

### 1. Unpack and read traces

- Extract the archive into an isolated working folder.
- Parse each `.ab1` file: base calls (`PBAS`), per-base Phred quality (`PCON`), trace data for the chromatogram, and sample name.
- Show per-sample stats like the Geneious view (`references/IMG_20261106_191522.jpg`): raw length, % HQ, % GC, and ambiguity count.

### 2. Remove primers

The ends of a read can contain PCR primer sites:

- **Single read:** it usually ends in the *opposite* primer's site, if the read runs that far. It does not start with its own sequencing primer, because that primer is not part of its own read.
- **Forward/reverse consensus:** it can contain both primer sites.

Primer sites must be removed before BLAST, for two reasons:

- **The primer bases are not from the sample.** They come from the synthetic primer that was copied into the PCR product. At degenerate or mismatched sites, the read shows the primer's base, not the specimen's base.
- **It is required for submission.** GenBank and BOLD expect the barcode without primers. For the standard Folmer region, that is 658 bp.

**Recommended method: find the primers by sequence, not by cutting a fixed number of bases.** A fixed cut (e.g. "remove the first 25 bp") breaks as soon as a read starts a few bases earlier or later, which is normal for Sanger reads.

1. **Choose the primer set.** Settings offers four options:

   | Option | What it does | When to use it |
   |--------|--------------|----------------|
   | **A. Auto-detect** (**recommended, default**) | Tests every read against all built-in primer sets and uses the best-matching set *for that read*. Shows a summary per set, such as *"LCO1490/HCO2198: 17 reads, none found: 3 reads"*, for the user to confirm before continuing. | You are not sure which primers were used, or batches come from different labs |
   | B. Preset | The user picks one set from the built-in library below | You know the primers |
   | C. Custom | The user enters their own primer names and sequences, which are saved for next time | The lab uses primers that are not in the library |
   | D. None | No primer removal. Only the HQ trim is applied, and exports are marked *primers not removed* | Primers were already removed by the sequencing service |

   **Built-in primer library** (all amplify the same ~658 bp Folmer barcode region of COI):

   | Set | Forward (5'→3') | Reverse (5'→3') | Typical use |
   |-----|-----------------|-----------------|-------------|
   | Folmer et al. 1994 | LCO1490 `GGTCAACAAATCATAAAGATATTGG` | HCO2198 `TAAACTTCAGGGTGACCAAAAAATCA` | The most common universal invertebrate pair; widely used for mosquitoes |
   | Meyer 2003 (degenerate) | dgLCO1490 `GGTCAACAAATCATAAAGAYATYGG` | dgHCO2198 `TAAACTTCAGGGTGACCAAARAAYCA` | Same site; works across more taxa |
   | Astrin & Stüben 2008 | LCO1490-JJ `CHACWAAYCATAAAGATATYGG` | HCO2198-JJ `AWACTTCVGGRTGVCCAAARAATCA` | Insects, when the Folmer pair amplifies poorly |
   | Hebert et al. 2004 | LepF1 `ATTCAACCAATCATAAAGATATTGG` | LepR1 `TAAACTTCTGGATGTCCAAAAAATCA` | Lepidoptera and many other insects; standard at BOLD |

   - IUPAC degenerate codes (`R`, `Y`, `W`, `H`, `V`, `N`, …) are supported.
   - M13-tailed versions of these primers need no separate entry. The tail sits outside the primer, so it is cut along with it.
   - The library sequences must be checked against the original papers before release.

   **Why auto-detect is recommended:**
   - It needs no prior knowledge.
   - It shows its evidence (how many reads matched each set), so the choice can be checked.
   - It catches a mixed batch.

   For this dataset the BLAST hits are *Aedes albopictus*, and the Folmer pair is the most likely result. Auto-detect confirms that from the reads instead of assuming it.

2. **Search both ends, in both orientations.**
   - Look for the forward primer near the 5' end and the reverse complement of the reverse primer near the 3' end, within the first/last ~100 bases.
   - Also look in the opposite orientation, so a reverse read is handled without the user flipping it first.
3. **Allow mismatches.** The search tries three rules in order:
   1. **Full primer:** up to 3 mismatches (adjustable), with at most 1 mismatch in the 5 bases at the primer's 3' end.
   2. **3' part only:** the 16 primer bases next to the barcode, with at most 1 substitution, insertion or deletion. At the end of a single read the far half of the site is often unreadable, while the part next to the barcode is still clean. In the example data this rule finds the HCO2198 site in 10 of the 20 reads, e.g. VN220, which has an extra base in `TTTTTT`.
   3. **Partial at the read edge:** at least 12 bases where the read stops inside the primer.

   A site is ignored if it would leave less than the minimum length (300 bp) of barcode. This removes false matches in noisy read starts.
4. **Cut the primer and everything outside it.** The kept region starts right after the forward primer and ends right before the reverse primer. Any adapter or leftover low-quality bases beyond the primer go with it.
5. **Search the raw read, before quality trimming.** Primer bases are often low quality. If quality trimming ran first, it could remove half a primer and leave the other half, which can no longer be recognised. So the primer positions set the outer limit, and HQ trimming (step 3) works only inside it.
6. **Report what was found.** Each read gets a status:
   - *found*: shows the position and the number of mismatches
   - *partial*: shows how many primer bases matched
   - *not found*

   A read is never cut blindly. *Not found* is normal at the 5' end of a single read, because the sequencing primer itself does not appear in its own read. It is also normal at the 3' end when the read stops before reaching the opposite primer.

   *Not found* is flagged only when the kept sequence is longer than the expected barcode length (see the length check below). That situation means a primer site is probably still inside it.
7. **Check the result.**
   - **Length:** the expected length depends on what the sequence is:
     - **Forward/reverse consensus:** close to the full barcode (658 bp for the Folmer region).
     - **Single read:** always shorter. The first ~20–40 bases after the sequencing primer cannot be read, so single reads normally give about 550–640 bp. In the example, VN220 keeps about 600 bp.

     The flag is raised only when the length is outside the range for that type (defaults: single read 450–658 bp, consensus 600–670 bp, both adjustable). A result longer than the barcode means a primer was not removed.
   - **Translation:** translate the barcode with the invertebrate mitochondrial genetic code (NCBI table 5) in the 3 forward reading frames, and look for stop codons. A trimmed read can start in the middle of a codon, so the frame is not known in advance. The check passes when at least one frame has no stop codon. A stop codon or frameshift inside COI usually means a sequencing error, an unremoved primer or adapter, or a nuclear pseudogene (NUMT). This is a standard check in barcoding QC.

**For F/R pairs:**
- The forward read usually ends in the reverse primer's site, and the reverse read ends in the forward primer's site.
- Each read is clipped in its own coordinates before assembly, so the consensus covers exactly the region between the two primers.

### 3. Trim to the high-quality region (HQ > 70%)

Raw reads start and end with low-quality bases. Only the good part of the nucleotide sequence is kept: the trimmed region must have **more than 70% high-quality bases**.

- **High-quality base:** Phred quality ≥ 40. Medium is 20–39 and low is < 20. This is believed to match Geneious's "% HQ" column (to verify, see [To verify](#to-verify)). The cut-off can be changed in Settings.
- **Trimming rule**, applied inside the primer limits from step 2:
  1. Trim low-quality bases from both ends using Mott's modified algorithm, with an error-probability limit of 0.05 (believed to be the Geneious default; to verify).
  2. Within what remains, keep the **longest continuous region whose % HQ is above 70%**. This is computed exactly, not by trimming one base at a time:
     - Score each base +0.3 if high quality and −0.7 if not.
     - A region is above 70% HQ exactly when its total score is above zero.
     - Find the longest such region with prefix sums. This runs in linear time, so it is instant even for a large batch.
     - If several regions tie, keep the one with the highest % HQ.
  3. **Ends must pass too.** Move each end inward until its outermost 20 bases are also above 70% HQ. Base-call errors cluster at read ends, so a region can pass overall while one end is weak.
  4. **Stop codons near an end.** COI has no stop codons. If every stop codon in the best reading frame lies within 80 bases of an end, that end is moved past them, since those bases are wrong.
     - Stops further inside are left alone and flagged.
     - The rule is skipped if it would leave less than the minimum length.
     - In the example, VN220 had 2 stop codons in its first ~20 kept bases; its quality scores there were 40–52, so only this rule catches it.
- Record the kept range (`bases X–Y`), trimmed length, and trimmed % HQ for each read.
- **Flag reads that cannot reach the threshold.** Some raw reads in the example are very low quality (e.g. one at 14.7% HQ). If no region of a useful length (default ≥ 300 bp) reaches > 70% HQ, the read is marked *Failed QC*. It is excluded from BLAST by default, but the user can include it anyway.
- Both the HQ threshold (70%) and the minimum length (300 bp) can be changed in Settings.

### 4. Manual review of trim limits

The automatic result is a starting point. The user can open any read and correct it on the chromatogram:

- The chromatogram shows coloured A/C/G/T traces and the base calls, each base coloured by quality (high / medium / low).
- Shaded areas mark the primers and the trimmed-off ends. The kept region is unshaded.
- **The left and right limits can be dragged.** % HQ, length, and the translation check update live as the limits move.
- *Reset* restores the automatic limits. Manually changed reads are marked *edited* in every table and export.
- Base calls themselves are not edited in this version. Only the trim limits change.

### 5. Read direction, pairing and assembly

Direction detection is in Phase 1. Pairing and assembly are in Phase 2 (see [Phases](#phases)).

Filenames cannot be relied on to say which read is forward and which is reverse. In the example data nothing marks direction: each sample appears once, and the `-1`/`-2`/`-3` suffixes are isolate numbers. So the app works out the direction from the sequence itself.

- **Detect the direction of every read automatically.** Two checks are combined:
  1. **Primer evidence.** Which primer site appears, and in which orientation, from the primer search in step 2. For example, the reverse complement of HCO2198 at the 3' end means a forward read.
  2. **Translation check.** COI is protein-coding. The read is translated in all 6 reading frames with the invertebrate mitochondrial code (table 5). The frame with no stop codons shows which strand the read is on.

  If the two checks disagree, or neither gives an answer, the read is marked *direction unknown* for the user to set by hand.
- **Put every output in the forward direction.** Reads detected as reverse are reverse-complemented. Every FASTA sequence and export then reads 5'→3' along the COI gene, which is how GenBank and BOLD expect it.
- **Treat files as single reads by default.** Every file is one sample unless it is paired.
- **Suggest pairs only with evidence.** Two files are suggested as an F/R pair when:
  - their names share the same sample stem,
  - their detected directions are opposite,
  - their sequences overlap.

  The user confirms or rejects each suggestion. Files can also be paired, unpaired, or swapped by hand. A filename pattern (e.g. `_F` / `_R`) can be set in Settings for future batches that do label direction, but it is optional.
- **Assembly:** for each pair:
  1. Reverse-complement the reverse read.
  2. Align it to the forward read.
  3. Build a consensus over the overlap. Where the two reads disagree, take the base with the higher quality.
  4. Flag a conflict when both bases are high quality.
- Report the overlap length, the number of disagreements, and the conflict positions.
- If a pair does not overlap, it is flagged, and the user chooses to BLAST the better read alone.

### 6. Convert to FASTA

- Write one combined multi-FASTA file, and optionally one `.fasta` per sample.
- **Sample IDs:**
  - NCBI gives every query its own ID (e.g. `Query_1885140`) and keeps the whole FASTA header as the query description. Results are matched back to samples by the first word of that description, with submission order as a fallback.
  - So the first word is always the sample name, taken from the filename without `.ab1`.
  - Spaces and unusual characters are replaced with `_`.
  - Duplicate names get a suffix (`_2`, `_3`), so every ID in a batch is unique.
- **Ambiguous bases:**
  - The final sequence keeps IUPAC ambiguity codes (`N`, `R`, `Y`, …) as called. They are never guessed or removed.
  - Their count is reported per sample.
  - The sample is flagged when ambiguous bases are more than 1% of its length (adjustable).
- The rest of the header describes how the sequence was built. In this illustration the ranges are made up; real values come from the app's own trimming:

```
>VN220 bases 77-677 primers:none-found/HCO2198
GTCGGAACTTCACTAAGAGTTTTAATTCGTATTGAACTTAGACATCCTGGTATATTTATT
GGAAATGATCAAATTTATAATGTAATTGTTACTGCTCATGCTTTTATTATAATTTTTTTT
...
>SAMPLE01 consensus F:12-690 R:20-701 overlap=640 primers:LCO1490/HCO2198
...
```

### 7. BLAST (online, NCBI)

Only the online NCBI service is used for now. There is no local BLAST+.

The search uses the same settings as the NCBI web form (`references/IMG_20261106_191917.jpg`):

| Setting      | Value                                         |
|--------------|-----------------------------------------------|
| Program      | `blastn` (Standard Nucleotide BLAST)          |
| Database     | `core_nt` (Core nucleotide database)          |
| Optimize for | Highly similar sequences (`megablast`)        |
| Max hits     | 100 per query                                 |
| Optional     | Organism filter, exclude models (XM/XP), exclude uncultured/environmental samples |

Searches go through NCBI's BLAST URL API (`https://blast.ncbi.nlm.nih.gov/Blast.cgi`):

1. **Put:** submit the FASTA and get back a Request ID (RID). Each submission holds a group of queries (see *Large batches* below).
2. **Poll:** check the RID's status until it is `READY`, with a progress indicator in the UI.
3. **Get:** download the results as single-file JSON (`FORMAT_TYPE=JSON2_S`).

NCBI usage rules that the app must follow:

- Submit no more than once every 10 seconds.
- Poll a RID no more than once per minute.
- Send `tool` and `email` parameters with each request. The email is entered once in Settings.
- Run large batches on weekends or between 9 pm and 5 am US Eastern time.

Results expire on NCBI **24 hours** after submission. The results page in the screenshot shows a search submitted around 19:12 that "expires on 11-07 19:12". To handle this:

- The app saves the raw response in the project as soon as it is downloaded, so results can be reopened at any time without re-running the search.
- If a RID expires before its results were downloaded (e.g. the app was closed), the app offers to resubmit the search.

**Large batches (up to 1000 samples).** NCBI's online service is shared and slow for big jobs, so the app splits the batch into groups:

- **Grouping:** 20 queries per submission by default (adjustable). 1000 samples become 50 submissions, sent one after another and never faster than NCBI's limits.
- **Expected time:** a 20-sample group usually takes a few minutes, but NCBI's load varies. A full 1000-sample batch can take **several hours**. The BLAST screen shows groups done / remaining and an estimated finish time.
- **Running in the background:** results appear group by group as they arrive, and the user can keep reviewing finished samples.
- **Resuming:** every RID and every finished group is saved in the project straight away. If the app or PC is closed, the next start resumes from the first unfinished group. Finished groups are never resent.
- **When a group fails:** only that group is retried. The rest of the batch continues.
- **Off-peak runs:** for batches above 100 samples, the app suggests NCBI's off-peak hours (weekends, or 9 pm–5 am US Eastern time, which is 9 am–5 pm in Indonesia, WIB).

**Results go out of date when a sequence changes.** If a sample's sequence changes after BLAST, its results are marked *outdated* everywhere, including exports. This can happen through a dragged trim limit, a different primer, or a changed direction. *Re-run BLAST* resubmits only the changed samples, not the whole batch.

### 8. BOLD Systems (optional, Phase 2)

**"Also search BOLD Systems"** is a checkbox on the BLAST screen, **unchecked by default**.

When checked, each sequence is also sent to the BOLD identification engine, the standard reference library for COI barcodes. Its results appear in a separate **BOLD** tab next to the NCBI results, with species, % similarity, and BIN.

**Risk:** BOLD has moved from its old v4 system to v5. Whether a public identification API exists now, and its terms and limits, must be confirmed before this feature is built (see [To verify](#to-verify)). If no suitable API exists, this option is dropped rather than built on scraping the website.

## Output

### Results summary (per sample)

Shown at the top of each result, as on NCBI (`references/IMG_20261106_191942.jpg`):

- Job title, RID, program, database
- Query ID, molecule type, query length

### Sequences producing significant alignments

The list of similar sequences, ranked by score (`references/IMG_20261106_191953.jpg`):

| Column          | Example                                                                                  |
|-----------------|------------------------------------------------------------------------------------------|
| Description     | Aedes albopictus isolate S-7 cytochrome c oxidase subunit I (COX1) gene, partial cds; mitochondrial |
| Scientific Name | Aedes albopictus                                                                         |
| Max Score       | 1175                                                                                     |
| Total Score     | 1175                                                                                     |
| Query Cover     | 100%                                                                                     |
| E value         | 0.0                                                                                      |
| Per. Ident      | 100.00%                                                                                  |
| Acc. Len        | 638                                                                                      |
| Accession       | PX112589.1 (opens GenBank in the browser)                                                |

The list can be sorted by any column and filtered by organism, percent identity, and E value.

### Alignment detail

Opens when a hit is selected (`references/IMG_20261106_192010.jpg`):

- Subject title, sequence ID, length, number of matches
- Range (e.g. `Range 1: 3 to 638`)
- Score (bits), Expect, Identities, Gaps, Strand
- Pairwise alignment, 60 bases per line, with `Query`/`Sbjct` positions and match bars:

```
Query  1    GTCGGAACTTCACTAAGAGTTTTAATTCGTATTGAACTTAGACATCCTGGTATATTTATT  60
            ||||||||||||||||||||||||||||||||||||||||||||||||||||||||||||
Sbjct  3    GTCGGAACTTCACTAAGAGTTTTAATTCGTATTGAACTTAGACATCCTGGTATATTTATT  62
```

Lowercase bases in the query are regions masked as low-complexity, as NCBI shows them.

### Batch overview (beyond what the NCBI page offers)

NCBI shows one query at a time. Since this app handles a whole batch, it adds one table with a row per sample:

| Sample | Reads (F/R/single) | Primers | Final length | % HQ | Stop codons | Top hit species | Per. Ident | Query Cover | Accession | Status |
|--------|--------------------|---------|--------------|------|-------------|-----------------|------------|-------------|-----------|--------|

**Status** flags samples that need review:
- *Failed QC*
- primer not found where expected
- unexpected length
- stop codon
- an F/R conflict
- low identity (e.g. < 97%, a common COI species threshold)
- mixed species among the top hits: more than one species among hits within 1% identity of the best hit
- too many ambiguous bases
- *edited*: trim limits were changed by hand
- *outdated*: the sequence changed after BLAST

### Compile results

A **Compile results** button on the Results screen answers one question for every sample: *which species is closest?* It looks at all of the sample's hits, up to 100 each, and the user does not have to read them.

**How the closest species is chosen, per sample:**

1. **Drop hits that cover only part of the sample.**
   - A hit counts only if its query cover is at least **80%** (adjustable in Settings).
   - Without this rule, a 100% match over just 50 bases would beat a 99.8% match over the full 600 bases.
   - This rule only decides which hits count. It is not a confidence threshold.
2. **Rank by similarity:** the hit with the **highest % identity** wins.
3. **Break exact ties:**
   1. higher query cover
   2. higher Max Score
   3. if still tied, the species backed by more hit records
4. **Always one answer.** The app always reports the single highest species, however small the gap to the next one.
   - The closest *different* species is shown next to it for information only (e.g. *runner-up: Aedes flavopictus, 99.1%*).
   - The runner-up never changes the answer.
5. **Species name:** taken from the hit's *Scientific Name*. Hits named only to genus (e.g. *Aedes sp.*) are reported exactly as named.

This ranking can differ from NCBI's own order. NCBI sorts hits by Max Score, so its top hit is not always the one with the highest % identity. Compile ranks by identity, as requested.

**Per-sample result table:**

| Sample | Closest species | % Identity | Query Cover | Accession | Runner-up species | Runner-up % | Hits counted | Status |
|--------|-----------------|------------|-------------|-----------|-------------------|-------------|--------------|--------|
| VN220  | Aedes albopictus | 100.00% | 100% | PX112589.1 | … | … | 100 | OK |

- *Status* reuses the batch overview flags, e.g. *Failed QC* or *outdated*.
- *No hit*: the sample has no hit with enough query cover.
- Samples that failed QC or were never searched are listed with their reason and left out of the chart.

**Batch chart:**

- A horizontal bar chart with one bar per species, showing how many samples have that species as their closest match. Bars are sorted from most to least.
- The bar length is the sample count; the label shows the count and its share, e.g. *Aedes albopictus: 14 (70%)*.
- **More than 15 species:** the 15 largest are shown and the rest are combined into *Other*, which expands on click.
- Clicking a bar filters the per-sample table to that species.
- Horizontal bars are used instead of a pie because they stay readable with many species and long Latin names.

**When it runs:**
- The user presses the button. It works on whatever results are available, so it can be pressed during a large batch run.
- With 1000 samples it finishes in a few seconds, because it reads the saved results and does not contact NCBI.
- If a sample's results change afterwards (*outdated*, or re-run), the compiled view shows *Recompile needed*.

## Export

All exports cover the whole batch and are saved through a standard Windows *Save as* dialog.

### Excel (`.xlsx`)

| Sheet       | Content                                                                     |
|-------------|-----------------------------------------------------------------------------|
| Overview    | The batch overview table, one row per sample, with status highlighted        |
| Compiled    | The compiled per-sample table (closest species, runner-up), plus the species chart as a native Excel chart (only after *Compile results* was run) |
| QC          | Per read: raw length, % HQ before/after, trim range, primer positions and mismatches, manual edit flag, F/R overlap and conflicts |
| Hits        | All BLAST hits for all samples (sample, rank, and every hit-list column), with filters on |
| Sequences   | Final barcode sequence per sample, ready to copy                             |
| BOLD        | BOLD results (only when the BOLD option was used)                            |
| Run info    | Date, app version, BLAST settings, RID, database, thresholds used            |

### PDF report (Phase 2)

- **Cover page:** batch name (defaults to the zip filename), date, the settings used (HQ threshold, primers, BLAST program and database), and RID.
- **Overview:** the batch overview table, the compiled species table, and the species chart.
- **Report size:** the user chooses *summary only*, the default for large batches, or *full report*. With 1000 samples a full report runs to over a thousand pages.
- **Per sample, one section each** (full report only):
  - QC summary (trim range, % HQ, primers, translation check)
  - the top 10 hits
  - the pairwise alignment of the best hit
- **Appendix:** the final sequences in FASTA.

## Projects: saving and reopening a batch

Each imported batch becomes a **project**, saved as a single `.coiproj` file. It is a zip archive the app manages, and it holds:

- a copy of the original `.ab1` files, so the project still works if the source zip is moved or deleted
- the settings used for this batch: primers, thresholds, BLAST options
- every per-read decision: primer positions, trim limits, manual edits, direction, pairs
- the final FASTA
- the raw NCBI (and BOLD) responses, plus RIDs and timestamps

Saving is automatic after every change. *File → Open* and a **recent projects** list on the start screen reopen a project exactly as it was left, including results. No network connection is needed to reopen.

**Two levels of settings:**
- The Settings screen holds the **defaults**, which are copied into each new project.
- Each project keeps its **own copy**. Changing it affects only that project, so an old batch always reopens with the settings it was processed with.

## Screens

0. **Start:** create a new project from a `.zip`, or open a recent one.
1. **Import:** after a `.zip` is chosen, the app shows the file list with:
   - per-read stats and QC status
   - the detected direction (F / R / unknown), which can be changed
   - suggested pairs, to confirm or reject
   - the primer auto-detect summary, to confirm
2. **Trim / QC:** the chromatogram with primers and trim limits, which can be dragged, plus live % HQ, length, and translation check.
3. **BLAST:** shows the search settings and the BOLD checkbox, then submits the search. Progress shows groups done / remaining and the estimated finish time.
4. **Results:**
   - the batch overview
   - for each sample: the summary, hit list, and alignment detail
   - the **Compile results** button, which opens the compiled per-sample table and the species chart
5. **Export:** choose Excel and/or PDF and where to save.
6. **Settings:**
   - NCBI email and language
   - HQ quality cut-off, HQ threshold, and minimum length
   - expected length ranges and the ambiguous-base limit
   - primer option (auto-detect / preset / custom / none), primer library, and allowed mismatches
   - optional F/R filename pattern
   - BLAST queries per submission (default 20)
   - Compile: minimum query cover for a hit to count (default 80%)

## Error handling

Problems are shown as clear messages on the affected sample or step. Nothing fails silently, and one bad file never stops the rest of the batch.

| Situation | What the app does |
|-----------|-------------------|
| Zip is corrupt, encrypted, or empty | Refuses the import and explains why |
| A file in the zip is not `.ab1`, or an `.ab1` cannot be parsed | Lists it as *skipped* with the reason; the other files continue |
| No internet connection | Steps 1–6 still work fully offline; BLAST waits and shows *offline*, with a *Retry* button |
| NCBI rejects the request, or reports too many requests | Waits and retries with increasing delays, never faster than NCBI's limits; shows the NCBI message if it keeps failing |
| NCBI search takes very long | Keeps polling once a minute and shows elapsed time; the user can cancel; the RID is saved so a closed app can continue later |
| RID expired before results were fetched | Offers to resubmit |
| NCBI returns no hits for a sample | Shows *No significant similarity found* (as NCBI does) and flags the sample |
| No email set in Settings | Asks for it before the first BLAST, because NCBI requires it |
| Disk full or project file not writable | Stops saving, keeps the work in memory, and asks for another location |

## Scope

**In scope:** everything described above, for Windows 10/11 only.

**Out of scope** (not planned for now):

- Editing individual base calls. Only trim limits can be changed.
- Local/offline BLAST (BLAST+) and custom local databases.
- Markers other than COI, e.g. 16S, ITS, cytb. The primer library, length checks and translation check are COI-specific.
- Phylogenetic trees, multiple alignment of the batch, and species delimitation.
- Submitting sequences to GenBank or BOLD.
- macOS and Linux builds.
- Multi-user and cloud features.

## Phases

All 20 example files are single reads, so Phase 1 is everything needed to process this kind of batch end to end. The rest follows in Phase 2.

| Phase | Contents |
|-------|----------|
| **Phase 1** | Import (up to 1000 samples); parse `.ab1`; primer auto-detect and removal; HQ > 70% trimming; manual trim on the chromatogram; read direction detection; FASTA; NCBI BLAST with grouped, resumable submission; results summary, hit list, alignment detail; batch overview; **Compile results** (closest species per sample and species chart); projects (save/reopen); Excel export; error handling |
| **Phase 2** | Forward/reverse pair suggestion and assembly; PDF report; BOLD search (only if the API check passes) |

## Definition of done (Phase 1)

Phase 1 is done when all of these pass on `COI_20_sequences.zip`:

1. **Import:** all 20 `.ab1` files parse. Raw length, % HQ and % GC match the Geneious values for the same files (see the reference data under To verify).
2. **Reference result reproduced:** the reference screenshots show a 638 bp query with **PX112589.1** (*Aedes albopictus* isolate S-7, COX1) as its top hit: Max Score 1175, Query Cover 100%, E value 0.0, 100.00% identity.
   - Submitting that same query sequence through the app must list PX112589.1 with the same values.
   - It does not have to stay the *top* hit. GenBank grows daily, so newer records may tie with it or rank above it.
   - Only part of the sequence is visible in the screenshot, so the full sequence and its sample name are needed (see the reference data under To verify).
3. **Same identification as the manual workflow:** for every sample that passes QC, the top-hit species matches the species from the manual Geneious + NCBI workflow. Any difference is explained by a QC flag (e.g. the app trimmed a poor read more strictly).
4. **Primers:** auto-detect reports a primer set and its supporting read count. No final sequence still contains a primer site.
5. **Nothing silently dropped:** every read either reaches BLAST or carries a visible reason (Failed QC, skipped, or excluded by the user).
6. **NCBI rules respected:** the full 20-sample batch completes in one session without breaking NCBI's rate limits. Request timing is checked in the app log.
7. **Excel:** the export opens in Microsoft Excel without repair warnings and contains every sheet listed under Export.
8. **Projects:** a saved project reopens with no network connection and shows the same trims, edits and results.
9. **Installer:** the `.exe` installer installs and runs on a clean Windows 10 and a clean Windows 11 machine.
10. **Compile results:** on the 20-sample batch, every searched sample gets exactly one closest species.
    - Each choice matches the rules: highest % identity among hits with ≥ 80% query cover, with the tie-breaks applied in order.
    - The bar chart's counts add up to the number of compiled samples.
11. **1000-sample scale:**
    - **Without NCBI:** a generated 1000-sample project, using saved BLAST results, compiles in under 10 seconds, and its tables scroll smoothly.
    - **With NCBI:** a real run of at least 100 samples completes in groups, survives closing and reopening the app mid-run, and never resends a finished group.

## To verify

These are assumptions written into this document that must be checked before or during Phase 1. Each one changes a default, not the design.

| # | Assumption | How to check |
|---|------------|--------------|
| 1 | Geneious "% HQ" counts bases with Phred ≥ 40 | **Verified.** Raw length, % HQ and % GC match the Geneious screenshot for all 16 visible samples (test `raw_stats_match_geneious`) |
| 2 | Geneious's default trim setting is an error-probability limit of 0.05 | Geneious documentation / trim dialog |
| 3 | The four built-in primer sequences are correct | Check against the original papers (Folmer 1994, Meyer 2003, Astrin & Stüben 2008, Hebert 2004) |
| 4 | NCBI RIDs expire after 24 hours | Observed in the reference screenshot; confirm in NCBI's URL API documentation |
| 5 | A public BOLD v5 identification API exists and allows this use | BOLD documentation and terms, before starting Phase 2 BOLD work |
| 6 | Which primers the lab actually used | Ask the lab. Until then, auto-detect decides |

**Found from the data:** the 638 bp screenshot query is **VN248-2 bases 43–680** (the Geneious trim). Submitted to NCBI on 2026-11-06, it gave PX112589.1 as hit #1 with 100.00% identity, 100% query cover, E value 0.0 and accession length 638. The bit score was 1176.0 as computed; NCBI's page showed 1175. The app shows bit scores rounded down, as NCBI's pages appear to do, so small differences like this are expected.

**Reference data needed from the Geneious workflow** (for the definition of done):
- a Geneious export of the 20 files' statistics (raw length, % HQ, % GC, trim ranges)
- the trimmed sequences
- the species identified for each sample

## Development

Requirements: Windows 10/11, Rust (stable), Node.js 20+, and the Visual Studio C++ build tools. pnpm comes with Node through corepack.

```powershell
corepack pnpm install
corepack pnpm tauri dev          # run the app
corepack pnpm test               # frontend unit tests
cd src-tauri; cargo test         # backend tests, including the example batch
corepack pnpm tauri build        # installer: src-tauri/target/release/bundle/nsis/
```

Code layout:

| Path | Content |
|------|---------|
| `src-tauri/src/abif.rs` | `.ab1` reader |
| `src-tauri/src/primers.rs` | Primer library and primer-site search |
| `src-tauri/src/trim.rs` | Mott trimming, HQ > 70% region, end rule |
| `src-tauri/src/analysis.rs` | Per-read pipeline: primers, trim, direction, QC flags |
| `src-tauri/src/blast.rs` | NCBI URL API client and JSON2_S parser |
| `src-tauri/src/compile.rs` | Closest species per sample, species counts |
| `src-tauri/src/store.rs` | `.coiproj` project files |
| `src-tauri/src/export.rs` | Excel, FASTA, raw results |
| `src-tauri/src/app.rs` | Tauri commands, background BLAST runner, auto-save |
| `src/screens/` | React screens |
| `src-tauri/tests/` | Tests against `COI_20_sequences.zip` and a real NCBI response (`tests/fixtures/ncbi_18_samples.json`) |

`src-tauri/examples/live_blast.rs` and `reference_check.rs` submit to the real NCBI service, for manual checks.

Setting `COI_BLAST_BASE_URL` points the app at another server instead of NCBI. This is only for testing against a mock server.

## Status (Phase 1)

| # | Definition of done | Status |
|---|--------------------|--------|
| 1 | Import; stats match Geneious | Passed: all 20 files parse; % HQ, % GC and length match |
| 2 | Reference result reproduced | Passed: PX112589.1 is hit #1 at 100% (see To verify) |
| 3 | Same identification as the manual workflow | Waiting for the Geneious reference data |
| 4 | Primers reported and removed | Passed: 19 of 20 reads have the HCO2198 site; none remains in a final sequence |
| 5 | Nothing silently dropped | Passed: VN222-2 and VN242-2 are shown as *Failed QC* |
| 6 | NCBI rules respected | Passed: polls 60 s apart, submissions one after another, `tool` and `email` sent |
| 7 | Excel opens without repair | Passed: opened in Microsoft Excel; 6 sheets and the chart |
| 8 | Project reopens offline | Passed |
| 9 | Installer on clean Windows 10 and 11 | Installer built; not yet tried on clean machines |
| 10 | Compile results | Passed: 18 samples, 5 species, counts add up |
| 11 | 1000-sample scale | Compile of 1000 samples takes 0.06 s. Resume after closing the app mid-run passed against a mock server. A real NCBI run of 100+ samples is still to do |
