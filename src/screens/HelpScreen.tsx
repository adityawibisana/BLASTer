import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink } from "lucide-react";
import { PageHeader } from "../components/ui";

const ISSUES = "https://github.com/adityawibisana/BLASTer/issues";

export const QUESTIONS: { issue: number; title: string; why: string; needed: string[] }[] = [
  {
    issue: 7,
    title: "Which primers did the lab use?",
    why: "Until we know, the app detects the primer set from the reads. Knowing the lab's primers lets us make them the default and check that they are removed correctly.",
    needed: ["The forward and reverse primer names and sequences used for the COI batch"],
  },
  {
    issue: 8,
    title: "Can you share your Geneious results for the example batch?",
    why: "We want to confirm that the app finds the same species as your manual Geneious + NCBI workflow for every sample.",
    needed: [
      "A Geneious export of the 20 files' statistics (raw length, % HQ, % GC, trim ranges)",
      "The trimmed sequences",
      "The species identified for each sample",
    ],
  },
];

export default function HelpScreen() {
  return (
    <div className="h-full overflow-auto">
      <PageHeader
        title="Help Us"
        subtitle="Open questions for the lab. Answering them helps us check the app against your own workflow."
      />
      <div className="flex max-w-[860px] flex-col gap-4 p-6">
        {QUESTIONS.map((q) => (
          <section key={q.issue} className="card flex flex-col gap-3 p-4">
            <h2 className="m-0 text-[14px] font-semibold">{q.title}</h2>
            <p className="m-0">{q.why}</p>
            <div>
              <div className="label mb-1">What we need</div>
              <ul className="m-0 list-disc pl-5">
                {q.needed.map((n) => (
                  <li key={n}>{n}</li>
                ))}
              </ul>
            </div>
            <div>
              <button className="btn btn-primary" onClick={() => openUrl(`${ISSUES}/${q.issue}`)}>
                <ExternalLink size={14} />
                Answer
              </button>
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}
