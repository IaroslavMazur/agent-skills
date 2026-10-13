import { RadioTower, SquareTerminal } from "lucide-react";
import { tv } from "tailwind-variants";

const errorPanel = tv({
  base: "rounded-md border border-danger/30 bg-danger-subtle px-4",
  variants: { compact: { true: "py-3", false: "py-8" } },
});
const errorContent = tv({
  base: "flex items-start gap-3",
  variants: { compact: { false: "flex-col" } },
});

export function EmptySessions() {
  return (
    <div className="border-y border-line bg-surface px-4 py-12 text-center">
      <span className="mx-auto flex size-10 items-center justify-center text-muted">
        <RadioTower aria-hidden="true" className="size-6" strokeWidth={1.5} />
      </span>
      <h3 className="mt-3 text-base font-semibold">No live agent sessions</h3>
      <p className="mx-auto mt-2 max-w-md text-[13px]/5 text-muted">
        New Codex and Claude Code sessions will appear here after their first coordination
        heartbeat.
      </p>
    </div>
  );
}

export function ApiErrorState({ detail, compact = false }: { detail: string; compact?: boolean }) {
  return (
    <div className={errorPanel({ compact })} role="alert">
      <div className={errorContent({ compact })}>
        <SquareTerminal
          aria-hidden="true"
          className="size-5 shrink-0 text-danger"
          strokeWidth={1.7}
        />
        <div>
          <h2 className="text-sm font-semibold">Dashboard API unreachable</h2>
          <p className="mt-1 text-[13px]/5 text-ink-secondary">
            Start the coordination API with{" "}
            <code className="font-mono text-danger">ai-coord serve</code>, then leave this page
            open.
          </p>
          <p className="mt-2 font-mono text-[11px]/5 wrap-anywhere text-muted">{detail}</p>
        </div>
      </div>
    </div>
  );
}
