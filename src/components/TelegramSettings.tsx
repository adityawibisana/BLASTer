import { useEffect, useState } from "react";
import { Bell, Eye, EyeOff } from "lucide-react";
import { toast } from "sonner";
import { api, errorText } from "../api";
import { Field } from "./ui";
import { MAX_TRIES } from "../lib/processes";
import type { NotifySettings } from "../types";

/** Optional Telegram messages about the BLAST searches. App-wide, saved on this computer. */
export default function TelegramSettings() {
  const [s, setS] = useState<NotifySettings | null>(null);
  const [show, setShow] = useState(false);
  const [busy, setBusy] = useState<"find" | "test" | "save" | null>(null);

  useEffect(() => {
    api.getNotifySettings().then(setS).catch((e) => toast.error(errorText(e)));
  }, []);
  if (!s) return null;
  const set = (patch: Partial<NotifySettings>) => setS({ ...s, ...patch });

  const act = async (what: "find" | "test" | "save", f: () => Promise<void>) => {
    setBusy(what);
    try {
      await f();
    } catch (e) {
      toast.error(errorText(e), { duration: 10_000 });
    } finally {
      setBusy(null);
    }
  };
  const find = () =>
    act("find", async () => {
      const chat = await api.telegramFindChat(s.botToken);
      set({ chatId: chat.id });
      toast.success(`Chat found${chat.name ? `: ${chat.name}` : ""}. Click Save to keep it.`);
    });
  const test = () => act("test", async () => {
    await api.telegramTest(s.botToken, s.chatId);
    toast.success("Test message sent. Check Telegram.");
  });
  const save = () =>
    act("save", async () => {
      const next = { ...s, enabled: s.enabled && !!s.botToken.trim() && !!s.chatId.trim() };
      await api.saveNotifySettings(next);
      setS(next);
      toast.success(next.enabled ? "Telegram notifications on" : "Telegram settings saved (notifications off)");
    });

  return (
    <section className="card col-span-2 flex flex-col gap-3 p-4" id="telegram">
      <div className="flex items-center justify-between gap-3">
        <h2 className="m-0 flex items-center gap-2 text-[14px] font-semibold">
          <Bell size={14} /> Telegram notifications <span className="label font-normal">(optional, for this computer)</span>
        </h2>
        <label className="flex cursor-pointer items-center gap-2">
          <input type="checkbox" checked={s.enabled} onChange={(e) => set({ enabled: e.target.checked })} />
          <span>Send messages</span>
        </label>
      </div>
      <div className="label">
        Get a message when all searches have finished, when a search times out, or when a group fails after all {MAX_TRIES} tries. Messages hold only
        the project name and counts, no sample names or sequences. Create a bot with @BotFather in Telegram to get its token.
      </div>
      <div className="grid grid-cols-2 gap-3">
        <Field label="Bot token (TELEGRAM_BOT_TOKEN)" hint="Saved only in this computer's app settings, never in a project file.">
          <div className="flex gap-2">
            <input
              className="input min-w-0 flex-1 font-mono"
              type={show ? "text" : "password"}
              autoComplete="off"
              spellCheck={false}
              placeholder="123456789:AA…"
              value={s.botToken}
              onChange={(e) => set({ botToken: e.target.value })}
            />
            <button type="button" className="btn btn-sm" onClick={() => setShow(!show)} title={show ? "Hide token" : "Show token"}>
              {show ? <EyeOff size={13} /> : <Eye size={13} />}
            </button>
          </div>
        </Field>
        <Field label="Chat ID" hint="Open your bot in Telegram, send /start, then click Find chat.">
          <div className="flex gap-2">
            <input className="input min-w-0 flex-1 font-mono" placeholder="e.g. 123456789" value={s.chatId} onChange={(e) => set({ chatId: e.target.value })} />
            <button type="button" className="btn btn-sm" onClick={find} disabled={!!busy || !s.botToken.trim()}>
              {busy === "find" ? "Looking…" : "Find chat"}
            </button>
          </div>
        </Field>
      </div>
      <div className="flex gap-2">
        <button type="button" className="btn btn-sm" onClick={test} disabled={!!busy || !s.botToken.trim() || !s.chatId.trim()}>
          {busy === "test" ? "Sending…" : "Send test message"}
        </button>
        <button type="button" className="btn btn-sm btn-primary" onClick={save} disabled={!!busy}>
          Save
        </button>
      </div>
    </section>
  );
}
