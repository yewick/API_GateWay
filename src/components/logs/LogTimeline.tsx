import type { ReactNode } from "react";
import {
  Bot,
  CheckCircle2,
  Server,
  User,
  Wrench,
  XCircle,
} from "lucide-react";
import type { ViewEvent } from "../../lib/logNormalize";

interface LogTimelineProps {
  events: ViewEvent[];
}

const icons: Record<ViewEvent["kind"], ReactNode> = {
  gateway: <Server size={14} />,
  user: <User size={14} />,
  assistant_text: <Bot size={14} />,
  tool_call: <Wrench size={14} />,
  tool_result: <CheckCircle2 size={14} />,
  response: <CheckCircle2 size={14} />,
  error: <XCircle size={14} />,
};

const toneClass = (event: ViewEvent) =>
  event.kind === "error"
    ? "text-danger border-danger/40 bg-danger/10"
    : event.kind === "response"
      ? "text-success border-success/40 bg-success/10"
      : event.kind === "tool_result"
        ? event.tone === "danger"
          ? "text-danger border-danger/40 bg-danger/10"
          : "text-success border-success/40 bg-success/10"
        : "text-text-secondary border-border-primary bg-bg-tertiary";

export function LogTimeline({ events }: LogTimelineProps) {
  return (
    <div className="relative space-y-3 pl-7">
      <div className="absolute bottom-2 left-[13px] top-2 w-px bg-border-primary" />
      {events.map((event, index) => (
        <div key={`${event.kind}-${index}`} className="relative">
          <span
            className={`absolute -left-7 top-0 flex h-[27px] w-[27px] items-center justify-center rounded-full border ${toneClass(event)}`}
          >
            {icons[event.kind]}
          </span>
          <div className="rounded-lg border border-border-primary bg-bg-secondary px-3 py-2">
            <div className="text-xs font-medium text-text-primary">
              {event.label}
            </div>
            {event.detail && (
              <div className="mt-0.5 text-[11px] text-text-muted break-all whitespace-pre-wrap">
                {event.detail}
              </div>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}
