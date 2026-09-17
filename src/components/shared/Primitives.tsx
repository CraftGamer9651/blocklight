import type { ReactNode } from "react";

export type BadgeTone = "neutral" | "success" | "warn" | "danger" | "info";

export function Badge({
  tone = "neutral",
  dot = false,
  children,
}: {
  tone?: BadgeTone;
  dot?: boolean;
  children: ReactNode;
}) {
  return (
    <span className={`badge badge-${tone}`}>
      {dot && <span className="status-dot" />}
      {children}
    </span>
  );
}

export function Button({
  variant = "secondary",
  size = "md",
  block = false,
  children,
  ...rest
}: {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "md" | "sm";
  block?: boolean;
  children: ReactNode;
} & React.ButtonHTMLAttributes<HTMLButtonElement>) {
  const classes = [
    "btn",
    `btn-${variant}`,
    size === "sm" ? "btn-sm" : "",
    block ? "btn-block" : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <button className={classes} {...rest}>
      {children}
    </button>
  );
}

export function StateBlock({
  title,
  body,
  error = false,
  action,
}: {
  title: string;
  body?: string;
  error?: boolean;
  action?: ReactNode;
}) {
  return (
    <div className={`state-block${error ? " is-error" : ""}`}>
      <div className="state-title">{title}</div>
      {body && <div className="state-body">{body}</div>}
      {action}
    </div>
  );
}

export function Spinner({ label }: { label?: string }) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        color: "var(--ink-dim)",
        fontSize: 13.5,
      }}
    >
      <span
        style={{
          width: 14,
          height: 14,
          borderRadius: "50%",
          border: "2px solid var(--line)",
          borderTopColor: "var(--ember)",
          animation: "spin 0.7s linear infinite",
          flexShrink: 0,
        }}
      />
      {label}
      <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
    </div>
  );
}
