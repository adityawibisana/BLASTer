import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { api, errorText } from "../api";
import type { ImportSummary } from "../types";

const list = (ids: string[], max = 3) => ids.slice(0, max).join(", ") + (ids.length > max ? ` and ${ids.length - max} more` : "");
const n = (count: number, one: string, many = `${one}s`) => `${count} ${count === 1 ? one : many}`;

/** What an import did, in words. `ok` is false when nothing was imported. */
export function importMessage(s: ImportSummary): { ok: boolean; text: string } {
  const notes: string[] = [];
  if (s.already.length) notes.push(`${n(s.already.length, "sample")} already had results and ${s.already.length === 1 ? "was" : "were"} left as ${s.already.length === 1 ? "it was" : "they were"}.`);
  if (s.changed.length)
    notes.push(`${n(s.changed.length, "sample")} changed since the file was searched (${list(s.changed)}); search them again.`);
  if (s.unknown.length) notes.push(`${n(s.unknown.length, "query", "queries")} in the file ${s.unknown.length === 1 ? "matches" : "match"} no sample of this project (${list(s.unknown)}).`);
  if (s.samples === 0) {
    const head = s.unknown.length && !s.already.length && !s.changed.length ? "Nothing imported: the file is for other samples." : "Nothing imported.";
    return { ok: false, text: [head, ...notes].join(" ") };
  }
  const head = `Results for ${n(s.samples, "sample")} imported.`;
  const left = s.stillMissing ? ` ${n(s.stillMissing, "sample")} still ${s.stillMissing === 1 ? "needs" : "need"} a search.` : " Every sample now has results.";
  return { ok: true, text: [head + left, ...notes].join(" ") };
}

/** Ask for a BLAST results file and import it. */
export async function pickAndImportResults(): Promise<ImportSummary | null> {
  const path = await open({
    multiple: false,
    filters: [{ name: "BLAST results (JSON, XML, zip)", extensions: ["json", "xml", "zip"] }],
  });
  if (typeof path !== "string") return null;
  try {
    const s = await api.importBlastResults(path);
    const m = importMessage(s);
    (m.ok ? toast.success : toast.warning)(m.text, { duration: 10_000 });
    return s;
  } catch (e) {
    toast.error(`Cannot import ${path.split(/[\\/]/).pop()}: ${errorText(e)}`);
    return null;
  }
}
