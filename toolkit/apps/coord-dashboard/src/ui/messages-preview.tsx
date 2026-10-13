import { ArrowRight, MessageSquareText } from "lucide-react";
import { AnimatePresence } from "motion/react";

import { orderMessages } from "@/lib/messages.js";
import type { Message } from "@/lib/types.js";
import { AnimatedValue } from "@/ui/animated-value.js";
import { MessageRow } from "@/ui/message-row.js";

const PREVIEW_SIZE = 5;

type MessagesPreviewProps = {
  messages: Message[];
  now: number;
  onOpenMessages: () => void;
};

export function MessagesPreview({ messages, now, onOpenMessages }: MessagesPreviewProps) {
  const latestMessages = orderMessages(messages).slice(0, PREVIEW_SIZE);
  const unreadCount = messages.filter((message) => message.acknowledged_at === null).length;

  return (
    <section
      aria-labelledby="messages-preview-heading"
      className="flex flex-col overflow-hidden rounded-2xl border border-line bg-surface shadow-panel xl:max-h-[calc(100dvh-3rem)]"
    >
      <div className="flex items-center justify-between gap-3 px-4 pt-4">
        <h2 id="messages-preview-heading" className="flex items-center gap-2 text-sm font-semibold">
          <MessageSquareText aria-hidden="true" className="size-4 text-accent" strokeWidth={1.8} />
          Latest messages
        </h2>
        <AnimatedValue
          className="rounded-md bg-surface-muted px-2 py-0.5 text-[11px] text-muted tabular-nums"
          value={unreadCount}
        >
          <span title={`${unreadCount} unread messages`}>{unreadCount} unread</span>
        </AnimatedValue>
      </div>
      <div className="min-h-0 overflow-y-auto px-4 py-2">
        {latestMessages.length === 0 ? (
          <p className="py-6 text-center text-xs text-muted">No coordination messages</p>
        ) : (
          <ol className="border-l border-line">
            <AnimatePresence initial={false} mode="popLayout">
              {latestMessages.map((message) => (
                <MessageRow compact key={message.id} message={message} now={now} showRepository />
              ))}
            </AnimatePresence>
          </ol>
        )}
      </div>
      <div className="border-t border-line-strong bg-surface-muted px-4 py-3">
        <button
          className="inline-flex h-8 w-full cursor-pointer items-center justify-center gap-1 rounded-lg border border-line bg-surface px-2.5 text-xs font-medium hover:bg-canvas focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
          onClick={onOpenMessages}
          type="button"
        >
          View all messages
          <ArrowRight aria-hidden="true" className="size-3.5" />
        </button>
      </div>
    </section>
  );
}
