import { ChevronLeft, ChevronRight, ListFilter, Search } from "lucide-react";
import { AnimatePresence } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { tv } from "tailwind-variants";

import { shortenPath } from "@/lib/format.js";
import { filterMessages, messageRepositories, paginateMessages } from "@/lib/messages.js";
import type { MessageStatusFilter } from "@/lib/messages.js";
import type { Message } from "@/lib/types.js";
import { AnimatedValue } from "@/ui/animated-value.js";
import { MessageRow } from "@/ui/message-row.js";

const statusButton = tv({
  base: "h-8 shrink-0 cursor-pointer rounded-md px-2.5 text-xs/4 transition-colors focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent motion-reduce:transition-none",
  variants: {
    selected: {
      true: "bg-accent-wash font-medium text-accent",
      false: "text-muted hover:bg-surface-muted hover:text-ink",
    },
  },
});

const statusFilters: {
  label: string;
  value: MessageStatusFilter;
}[] = [
  { label: "All", value: "all" },
  { label: "Unread", value: "unread" },
  { label: "Acknowledged", value: "acknowledged" },
];

type MessagesFeedProps = {
  messages: Message[];
  now: number;
};

export function MessagesFeed({ messages, now }: MessagesFeedProps) {
  const [query, setQuery] = useState("");
  const [repoRoot, setRepoRoot] = useState<string | null>(null);
  const [status, setStatus] = useState<MessageStatusFilter>("all");
  const [page, setPage] = useState(1);
  const resultsRef = useRef<HTMLElement>(null);
  const repositories = messageRepositories(messages);
  const filteredMessages = filterMessages(messages, { query, repoRoot, status });
  const messagePage = paginateMessages(filteredMessages, page);

  useEffect(() => {
    if (messagePage.page !== page) {
      setPage(messagePage.page);
      resultsRef.current?.scrollIntoView({ block: "start" });
    }
  }, [messagePage.page, page]);

  const showPage = (nextPage: number) => {
    setPage(nextPage);
    resultsRef.current?.scrollIntoView({ block: "start" });
  };

  const resetFilters = () => {
    setQuery("");
    setRepoRoot(null);
    setStatus("all");
    showPage(1);
  };

  return (
    <section aria-labelledby="messages-heading" className="min-w-0 scroll-mt-4" ref={resultsRef}>
      <div className="flex items-center justify-between gap-3">
        <h2 id="messages-heading" className="text-xl font-semibold tracking-tight">
          Messages
        </h2>
        <AnimatedValue className="text-xs text-muted tabular-nums" value={messages.length}>
          {messages.length}
        </AnimatedValue>
      </div>
      <p className="mt-1 mb-5 text-sm/5 text-muted">
        Agent conversations retained in the local coordination ledger.
      </p>
      <div className="grid gap-3 border-y border-line py-3 lg:grid-cols-[minmax(15rem,1fr)_auto_minmax(12rem,16rem)] lg:items-center">
        <label className="relative block min-w-0">
          <span className="sr-only">Search messages</span>
          <Search
            aria-hidden="true"
            className="pointer-events-none absolute top-1/2 left-3 size-3.5 -translate-y-1/2 text-muted"
          />
          <input
            className="h-9 w-full rounded-md border border-line bg-canvas pr-3 pl-9 text-xs text-ink outline-hidden placeholder:text-muted focus:border-accent focus:outline-2 focus:outline-offset-1 focus:outline-accent"
            onChange={(event) => {
              setQuery(event.target.value);
              setPage(1);
            }}
            placeholder="Search messages, agents, or paths"
            type="search"
            value={query}
          />
        </label>

        <div aria-label="Message status" className="flex min-w-0 flex-wrap gap-1" role="group">
          {statusFilters.map((filter) => {
            const selected = status === filter.value;
            return (
              <button
                aria-pressed={selected}
                className={statusButton({ selected })}
                key={filter.value}
                onClick={() => {
                  setStatus(filter.value);
                  setPage(1);
                }}
                type="button"
              >
                {filter.label}
              </button>
            );
          })}
        </div>

        <label className="flex min-w-0 items-center gap-2">
          <ListFilter aria-hidden="true" className="size-3.5 shrink-0 text-muted" />
          <span className="sr-only">Repository</span>
          <select
            className="h-9 min-w-0 flex-1 rounded-md border border-line bg-canvas px-2 text-xs/4 text-ink outline-hidden focus:border-accent focus:outline-2 focus:outline-offset-1 focus:outline-accent"
            onChange={(event) => {
              setRepoRoot(event.target.value || null);
              setPage(1);
            }}
            value={repoRoot ?? ""}
          >
            <option value="">All repositories</option>
            {repositories.map((repository) => (
              <option key={repository} value={repository}>
                {shortenPath(repository)}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="min-w-0">
        {messagePage.items.length === 0 ? (
          <div className="flex min-h-56 items-center justify-center border-y border-line-muted px-4 text-center">
            <div>
              <p className="text-sm font-medium">
                {messages.length === 0
                  ? "No coordination messages"
                  : "No messages match these filters"}
              </p>
              <button
                className="mt-2 cursor-pointer text-xs font-medium text-accent hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
                onClick={resetFilters}
                type="button"
              >
                Clear filters
              </button>
            </div>
          </div>
        ) : (
          <ol className="divide-y divide-line">
            <AnimatePresence initial={false} mode="popLayout">
              {messagePage.items.map((message) => (
                <MessageRow key={message.id} message={message} now={now} showRepository />
              ))}
            </AnimatePresence>
          </ol>
        )}
      </div>
      <div className="flex flex-wrap items-center justify-between gap-3 border-t border-line py-3">
        <p aria-live="polite" className="text-xs/4 text-muted tabular-nums">
          {messagePage.start}–{messagePage.end} of {messagePage.total}
        </p>
        <div className="flex items-center gap-2">
          <button
            className="inline-flex h-8 cursor-pointer items-center gap-1 rounded-md border border-line bg-surface px-2.5 text-xs font-medium focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent enabled:hover:bg-canvas disabled:cursor-not-allowed disabled:opacity-40"
            disabled={messagePage.page === 1}
            onClick={() => showPage(messagePage.page - 1)}
            type="button"
          >
            <ChevronLeft aria-hidden="true" className="size-3.5" />
            Previous
          </button>
          <span className="text-xs/4 text-muted tabular-nums">
            {messagePage.page}/{messagePage.pageCount}
          </span>
          <button
            className="inline-flex h-8 cursor-pointer items-center gap-1 rounded-md border border-line bg-surface px-2.5 text-xs font-medium focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent enabled:hover:bg-canvas disabled:cursor-not-allowed disabled:opacity-40"
            disabled={messagePage.page === messagePage.pageCount}
            onClick={() => showPage(messagePage.page + 1)}
            type="button"
          >
            Next
            <ChevronRight aria-hidden="true" className="size-3.5" />
          </button>
        </div>
      </div>
    </section>
  );
}
