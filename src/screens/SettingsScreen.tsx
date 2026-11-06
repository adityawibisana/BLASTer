import { useEffect, useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { api, errorText } from "../api";
import i18n from "../i18n";
import { useApp } from "../state";
import { Field, PageHeader } from "../components/ui";
import type { PrimerMode, PrimerSet, Settings } from "../types";

function Num({ value, onChange, step = 1, min, max }: { value: number; onChange: (v: number) => void; step?: number; min?: number; max?: number }) {
  return (
    <input
      className="input w-[110px]"
      type="number"
      step={step}
      min={min}
      max={max}
      value={Number.isFinite(value) ? value : ""}
      onChange={(e) => onChange(Number(e.target.value))}
    />
  );
}

const EMPTY_SET: PrimerSet = { name: "", forwardName: "", forward: "", reverseName: "", reverse: "" };

export default function SettingsScreen() {
  const { project, setProject } = useApp();
  const [draft, setDraft] = useState<Settings | null>(null);
  const [library, setLibrary] = useState<PrimerSet[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    api.primerLibrary().then(setLibrary).catch(() => {});
  }, []);
  useEffect(() => {
    if (project) setDraft(structuredClone(project.settings));
    else api.getDefaults().then(setDraft).catch((e) => toast.error(errorText(e)));
  }, [project?.path, project?.settings]);

  if (!draft) return null;
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setDraft({ ...draft, [k]: v });
  const setCustom = (i: number, patch: Partial<PrimerSet>) =>
    set("customPrimers", draft.customPrimers.map((p, j) => (j === i ? { ...p, ...patch } : p)));

  const validate = (): string | null => {
    if (draft.hqThreshold <= 0 || draft.hqThreshold >= 100) return "HQ threshold must be between 0 and 100.";
    if (draft.minLength < 50) return "Minimum length must be at least 50 bases.";
    if (draft.singleLenMin > draft.singleLenMax) return "Expected length: the minimum is above the maximum.";
    for (const p of draft.customPrimers) {
      if (!p.name.trim()) return "Every custom primer set needs a name.";
      if (!/^[ACGTURYSWKMBDHVN]+$/i.test(p.forward) || !/^[ACGTURYSWKMBDHVN]+$/i.test(p.reverse))
        return `Primer set "${p.name}": sequences may only contain IUPAC codes (A C G T R Y S W K M B D H V N).`;
    }
    if (draft.primerMode === "custom" && draft.customPrimers.length === 0) return "Add at least one custom primer set.";
    return null;
  };

  const apply = async () => {
    const problem = validate();
    if (problem) {
      toast.error(problem);
      return;
    }
    setSaving(true);
    try {
      i18n.changeLanguage(draft.language);
      if (project) {
        setProject(await api.updateSettings(draft));
        toast.success("Settings applied to this project. Reads were re-analysed; manual edits are kept.");
      } else {
        await api.saveDefaults(draft);
        toast.success("Defaults saved");
      }
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setSaving(false);
    }
  };

  const saveAsDefaults = async () => {
    const problem = validate();
    if (problem) return toast.error(problem);
    await api.saveDefaults(draft).then(() => toast.success("Saved as defaults for new projects"), (e) => toast.error(errorText(e)));
  };

  const useDefaults = async () => {
    const d = await api.getDefaults();
    setDraft({ ...d, email: d.email || draft.email });
  };

  return (
    <div className="h-full overflow-auto">
      <PageHeader
        title="Settings"
        subtitle={
          project
            ? `Settings of project “${project.name}”. New projects start from the defaults.`
            : "Defaults, copied into every new project."
        }
        actions={
          <>
            {project && (
              <>
                <button className="btn" onClick={useDefaults}>
                  Load defaults
                </button>
                <button className="btn" onClick={saveAsDefaults}>
                  Save as defaults
                </button>
              </>
            )}
            <button className="btn btn-primary" onClick={apply} disabled={saving || project?.blastRunning}>
              {project ? "Apply to project" : "Save defaults"}
            </button>
          </>
        }
      />
      <div className="grid max-w-[1000px] grid-cols-2 gap-4 px-6 pb-8">
        <section className="card flex flex-col gap-3 p-4">
          <h2 className="m-0 text-[14px] font-semibold">General</h2>
          <Field label="NCBI email" hint="Required by NCBI for every BLAST request; sent only to NCBI.">
            <input className="input" value={draft.email} onChange={(e) => set("email", e.target.value)} placeholder="name@institution.ac.id" />
          </Field>
          <Field label="Language">
            <select className="input w-[200px]" value={draft.language} onChange={(e) => set("language", e.target.value)}>
              <option value="en">English</option>
              <option value="id">Bahasa Indonesia</option>
            </select>
          </Field>
        </section>

        <section className="card flex flex-col gap-3 p-4">
          <h2 className="m-0 text-[14px] font-semibold">Quality trimming</h2>
          <div className="grid grid-cols-2 gap-3">
            <Field label="High-quality base" hint="Phred ≥ (40 matches Geneious % HQ)">
              <Num value={draft.hqQuality} onChange={(v) => set("hqQuality", v)} min={1} max={60} />
            </Field>
            <Field label="HQ threshold (%)" hint="Kept region must be above this">
              <Num value={draft.hqThreshold} onChange={(v) => set("hqThreshold", v)} step={1} min={1} max={99} />
            </Field>
            <Field label="Minimum length (bp)" hint="Shorter reads fail QC">
              <Num value={draft.minLength} onChange={(v) => set("minLength", v)} min={50} />
            </Field>
            <Field label="Mott error limit">
              <Num value={draft.mottCutoff} onChange={(v) => set("mottCutoff", v)} step={0.01} min={0.001} max={0.5} />
            </Field>
            <Field label="Expected length, single read (bp)">
              <div className="flex items-center gap-1.5">
                <Num value={draft.singleLenMin} onChange={(v) => set("singleLenMin", v)} />
                <span>–</span>
                <Num value={draft.singleLenMax} onChange={(v) => set("singleLenMax", v)} />
              </div>
            </Field>
            <Field label="Ambiguous bases limit (%)">
              <Num value={draft.ambiguityMaxPct} onChange={(v) => set("ambiguityMaxPct", v)} step={0.1} min={0} />
            </Field>
          </div>
        </section>

        <section className="card col-span-2 flex flex-col gap-3 p-4">
          <h2 className="m-0 text-[14px] font-semibold">Primers</h2>
          <div className="flex flex-wrap gap-4">
            {(
              [
                ["auto", "Auto-detect (recommended)", "Every read is tested against all sets"],
                ["preset", "Preset", "One set from the library"],
                ["custom", "Custom", "Your own primer sets"],
                ["none", "None", "No primer removal"],
              ] as [PrimerMode, string, string][]
            ).map(([mode, label, hint]) => (
              <label key={mode} className="card flex w-[220px] cursor-pointer items-start gap-2 p-3">
                <input type="radio" name="pm" className="mt-1" checked={draft.primerMode === mode} onChange={() => set("primerMode", mode)} />
                <span>
                  <span className="font-medium">{label}</span>
                  <span className="label block">{hint}</span>
                </span>
              </label>
            ))}
          </div>
          {draft.primerMode === "preset" && (
            <Field label="Preset set">
              <select className="input w-[320px]" value={draft.presetSet} onChange={(e) => set("presetSet", e.target.value)}>
                {[...library, ...draft.customPrimers].map((p) => (
                  <option key={p.name} value={p.name}>
                    {p.name} ({p.forwardName} / {p.reverseName})
                  </option>
                ))}
              </select>
            </Field>
          )}
          <div className="grid grid-cols-2 gap-3">
            <Field label="Allowed mismatches per primer">
              <Num value={draft.maxMismatches} onChange={(v) => set("maxMismatches", v)} min={0} max={8} />
            </Field>
            <Field label="Minimum partial primer (bases)" hint="Primer cut off by the read end">
              <Num value={draft.minPartial} onChange={(v) => set("minPartial", v)} min={8} max={25} />
            </Field>
          </div>

          <div className="mt-1 font-medium">Built-in library</div>
          <div className="overflow-hidden rounded-md border border-line">
            {library.map((p) => (
              <div key={p.name} className="grid grid-cols-[180px_1fr_1fr] gap-3 border-b border-line px-3 py-1.5 last:border-0">
                <span>{p.name}</span>
                <span className="font-mono text-[12px]">
                  {p.forwardName} {p.forward}
                </span>
                <span className="font-mono text-[12px]">
                  {p.reverseName} {p.reverse}
                </span>
              </div>
            ))}
          </div>

          <div className="mt-1 flex items-center justify-between">
            <div className="font-medium">Custom primer sets</div>
            <button className="btn btn-sm" onClick={() => set("customPrimers", [...draft.customPrimers, { ...EMPTY_SET }])}>
              <Plus size={13} /> Add set
            </button>
          </div>
          {draft.customPrimers.length === 0 && <div className="label">None. Custom sets are also searched by auto-detect.</div>}
          {draft.customPrimers.map((p, i) => (
            <div key={i} className="grid grid-cols-[150px_100px_1fr_100px_1fr_auto] items-center gap-2">
              <input className="input" placeholder="Set name" value={p.name} onChange={(e) => setCustom(i, { name: e.target.value })} />
              <input className="input" placeholder="Fwd name" value={p.forwardName} onChange={(e) => setCustom(i, { forwardName: e.target.value })} />
              <input className="input font-mono" placeholder="Forward 5'→3'" value={p.forward} onChange={(e) => setCustom(i, { forward: e.target.value.toUpperCase() })} />
              <input className="input" placeholder="Rev name" value={p.reverseName} onChange={(e) => setCustom(i, { reverseName: e.target.value })} />
              <input className="input font-mono" placeholder="Reverse 5'→3'" value={p.reverse} onChange={(e) => setCustom(i, { reverse: e.target.value.toUpperCase() })} />
              <button className="btn btn-sm" onClick={() => set("customPrimers", draft.customPrimers.filter((_, j) => j !== i))}>
                <Trash2 size={13} />
              </button>
            </div>
          ))}
        </section>

        <section className="card flex flex-col gap-3 p-4">
          <h2 className="m-0 text-[14px] font-semibold">BLAST</h2>
          <Field label="Samples per submission" hint="Searches are sent one group after another">
            <Num value={draft.blast.groupSize} onChange={(v) => set("blast", { ...draft.blast, groupSize: v })} min={1} max={100} />
          </Field>
          <Field label="Max hits per sample">
            <Num value={draft.blast.maxHits} onChange={(v) => set("blast", { ...draft.blast, maxHits: v })} min={10} max={500} />
          </Field>
          <Field label="Low identity flag below (%)">
            <Num value={draft.lowIdentity} onChange={(v) => set("lowIdentity", v)} step={0.5} min={50} max={100} />
          </Field>
        </section>

        <section className="card flex flex-col gap-3 p-4">
          <h2 className="m-0 text-[14px] font-semibold">Compile results</h2>
          <Field label="Minimum query cover for a hit to count (%)" hint="Stops a short 100% match from beating a full-length one">
            <Num value={draft.compileMinCover} onChange={(v) => set("compileMinCover", v)} min={0} max={100} />
          </Field>
        </section>
      </div>
    </div>
  );
}
