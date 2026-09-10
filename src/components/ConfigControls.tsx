import { type ReactNode } from "react";
import { CircleDot } from "lucide-react";

export function StatusDot({ alive }: { alive: boolean }) {
  return <span className={alive ? "status-dot is-live" : "status-dot is-off"} aria-hidden="true" />;
}

export function TogglePill({
  checked,
  label,
  small,
  disabled,
  onChange,
}: {
  checked: boolean;
  label: string;
  small?: boolean;
  disabled?: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <button
      className={`toggle-pill${small ? " is-small" : ""}${checked ? " is-on" : " is-off"}`}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    >
      <StatusDot alive={checked} />
      {checked ? "已启用" : "已停用"}
    </button>
  );
}

export function SectionTitle({
  icon,
  children,
  action,
}: {
  icon: ReactNode;
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="section-head">
      <span className="section-title">
        {icon}
        {children}
      </span>
      {action}
    </div>
  );
}

export function GlyphButton({
  label,
  onClick,
  disabled,
  danger,
  children,
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  danger?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      className={danger ? "glyph-button is-danger" : "glyph-button"}
      type="button"
      title={label}
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

export function InlineError({ message }: { message: string }) {
  return (
    <p className="inline-error" role="alert">
      {message}
    </p>
  );
}

export function LoadingLines({ rows = 3 }: { rows?: number }) {
  return (
    <div className="loading-lines" aria-hidden="true">
      {Array.from({ length: rows }, (_, index) => (
        <span key={index} />
      ))}
    </div>
  );
}

export function FormActions({
  busy,
  submitLabel,
  onCancel,
}: {
  busy: boolean;
  submitLabel: string;
  onCancel: () => void;
}) {
  return (
    <div className="form-actions">
      <button className="quiet-button is-primary" type="submit" disabled={busy}>
        {busy ? "保存中…" : submitLabel}
      </button>
      <button className="quiet-button" type="button" onClick={onCancel} disabled={busy}>
        取消
      </button>
    </div>
  );
}

export function SaveBar({
  dirty,
  busy,
  label,
  onDiscard,
}: {
  dirty: boolean;
  busy: boolean;
  label: string;
  onDiscard: () => void;
}) {
  if (!dirty) return null;
  return (
    <div className="save-bar">
      <span className="save-bar-note">
        <CircleDot aria-hidden="true" />
        有未保存的修改
      </span>
      <div className="save-bar-actions">
        <button className="quiet-button" type="button" onClick={onDiscard} disabled={busy}>
          放弃
        </button>
        <button className="quiet-button is-primary" type="submit" disabled={busy}>
          {busy ? "保存中…" : label}
        </button>
      </div>
    </div>
  );
}

export function EmptyNote({ children }: { children: ReactNode }) {
  return <p className="empty-note">{children}</p>;
}
