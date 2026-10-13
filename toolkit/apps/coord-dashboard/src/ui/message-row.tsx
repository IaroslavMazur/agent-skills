import { ArrowRight, Check } from "lucide-react";
import { motion } from "motion/react";
import { tv } from "tailwind-variants";

import { displayPath, formatRelativeTime, messageEndpointName, shortenPath } from "@/lib/format.js";
import { MOTION_DURATION, MOTION_EASE } from "@/lib/motion.js";
import type { Message } from "@/lib/types.js";
import { AnimatedValue } from "@/ui/animated-value.js";

const messageDot = tv({
  base: "size-1.5 shrink-0 rounded-full transition-colors motion-reduce:transition-none",
  variants: { acknowledged: { true: "bg-muted", false: "bg-accent" } },
});
const messageText = tv({
  base: "mt-2 text-sm/6 wrap-anywhere text-ink-secondary",
  variants: {
    compact: { true: "line-clamp-3 text-[13px]/5", false: "wrap-break-word whitespace-pre-wrap" },
  },
});

type MessageRowProps = {
  compact?: boolean;
  message: Message;
  now: number;
  showRepository?: boolean;
};

export function MessageRow({
  compact = false,
  message,
  now,
  showRepository = false,
}: MessageRowProps) {
  const acknowledged = message.acknowledged_at !== null;
  const senderLabel = messageEndpointName(message.sender_callsign, message.sender_session_id);
  const recipientLabel = messageEndpointName(
    message.recipient_callsign,
    message.recipient_session_id
  );

  return (
    <motion.li
      animate={{ opacity: 1, y: 0 }}
      className="min-w-0 py-4"
      data-motion-item
      exit={{ opacity: 0, y: -6 }}
      initial={{ opacity: 0, y: 8 }}
      layout="position"
      transition={{
        duration: MOTION_DURATION.row,
        ease: MOTION_EASE,
        layout: { duration: MOTION_DURATION.layout, ease: MOTION_EASE },
      }}
    >
      <div className="flex min-w-0 flex-wrap items-center gap-2 text-xs/5">
        <span className={messageDot({ acknowledged })} aria-hidden="true" />
        <AnimatedValue className="min-w-0" value={senderLabel}>
          <span className="block font-semibold wrap-anywhere">{senderLabel}</span>
        </AnimatedValue>
        <ArrowRight aria-hidden="true" className="size-3 shrink-0 text-muted" />
        <AnimatedValue className="min-w-0" value={recipientLabel}>
          <span className="block wrap-anywhere">{recipientLabel}</span>
        </AnimatedValue>
      </div>
      {showRepository && message.repo_root ? (
        <p
          className="mt-1 truncate font-mono text-[11px]/5 text-muted"
          title={displayPath(message.repo_root)}
        >
          {shortenPath(message.repo_root)}
        </p>
      ) : null}
      <p className={messageText({ compact })} title={compact ? message.text : undefined}>
        {message.text}
      </p>
      <div className="mt-2 flex flex-wrap items-center justify-between gap-x-3 gap-y-1 text-[11px]/5 text-muted">
        <span>{formatRelativeTime(message.created_at, now)}</span>
        <AnimatedValue value={acknowledged}>
          <span className="inline-flex items-center gap-1">
            {acknowledged ? <Check aria-hidden="true" className="size-3" /> : null}
            {acknowledged ? "Acknowledged" : "Unread"}
          </span>
        </AnimatedValue>
      </div>
    </motion.li>
  );
}
